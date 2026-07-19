//! `WorkerService` implementation: the koine.v1 wire contract (ADR 0013)
//! wired onto the phase-1 use cases, authenticated per ADR 0014.

use std::sync::Arc;
use std::time::Duration;

use koine_application::ports::{
    Clock, DispatchSignal, Dispatcher, EventStore, EventStoreError, IdGenerator, WorkerPresence,
};
use koine_application::use_cases::heartbeat::Heartbeat;
use koine_application::use_cases::lease::LeaseNextJob;
use koine_application::use_cases::worker_ack::{AckError, AckOutcome, WorkerAck};
use koine_domain::{JobId, LeaseId, QueueName};
use koine_proto::v1;
use koine_proto::v1::worker_service_server::{WorkerService, WorkerServiceServer};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::auth;

/// Static gRPC server configuration (ADR 0014): the shared bearer token plus
/// the fetch-stream's lease and polling knobs.
pub struct GrpcConfig {
    /// The bearer token every worker must present.
    pub token: String,
    /// Ceiling every requested lease TTL is clamped to.
    pub max_lease_ttl: Duration,
    /// How long a drained `Fetch` poll waits for a dispatch signal before
    /// re-checking the queue.
    pub idle_poll: Duration,
}

/// The driven ports plus static configuration the worker gRPC surface is
/// built from.
pub struct Deps<S, D, G, C, Sig, P> {
    /// Event store port.
    pub store: S,
    /// Dispatcher port.
    pub dispatcher: D,
    /// Id source.
    pub ids: G,
    /// Time source.
    pub clock: C,
    /// Dispatch wakeup signal.
    pub signal: Sig,
    /// Worker presence tracker.
    pub presence: P,
    /// Static server configuration.
    pub config: GrpcConfig,
}

/// `WorkerService` implementation bridging the koine.v1 wire contract to the
/// phase-1 use cases.
pub struct WorkerApi<S, D, G, C, Sig, P> {
    deps: Arc<Deps<S, D, G, C, Sig, P>>,
}

impl<S, D, G, C, Sig, P> WorkerApi<S, D, G, C, Sig, P> {
    /// Wraps `deps` as the gRPC service implementation.
    #[must_use]
    pub fn new(deps: Arc<Deps<S, D, G, C, Sig, P>>) -> Self {
        Self { deps }
    }
}

/// Builds the tonic server type for `WorkerApi`, ready to add to a `Server`.
pub fn server<S, D, G, C, Sig, P>(
    deps: Arc<Deps<S, D, G, C, Sig, P>>,
) -> WorkerServiceServer<WorkerApi<S, D, G, C, Sig, P>>
where
    S: EventStore + 'static,
    D: Dispatcher + 'static,
    G: IdGenerator + 'static,
    C: Clock + 'static,
    Sig: DispatchSignal + 'static,
    P: WorkerPresence + 'static,
{
    WorkerServiceServer::new(WorkerApi::new(deps))
}

/// Clamps a requested lease TTL (wire milliseconds) to the configured
/// ceiling.
///
/// # Errors
/// `InvalidArgument` if the requested TTL is zero.
fn clamp_lease_ttl(requested_ms: u64, max: Duration) -> Result<Duration, Status> {
    if requested_ms == 0 {
        return Err(Status::invalid_argument(
            "lease_ttl_ms must be greater than zero",
        ));
    }
    Ok(Duration::from_millis(requested_ms).min(max))
}

/// Parses a wire UUID field, naming it in the error on failure.
fn parse_uuid(field: &str, raw: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|_| Status::invalid_argument(format!("invalid {field}")))
}

/// Maps a `WorkerAck` failure to the wire status (ADR 0013): a domain
/// rejection means the worker's view is stale and it must refetch; a missing
/// stream is `not_found`; any other store failure is an opaque `internal`.
fn map_ack_error(err: &AckError) -> Status {
    match err {
        AckError::Domain(_) => {
            Status::failed_precondition("job state no longer permits this operation; refetch")
        }
        AckError::Store(EventStoreError::StreamNotFound(_)) => Status::not_found("job not found"),
        AckError::Store(_) => Status::internal("store error"),
    }
}

/// Maps the use-case ack outcome to the wire enum.
fn to_proto_outcome(outcome: AckOutcome) -> i32 {
    match outcome {
        AckOutcome::Recorded => v1::AckOutcome::Recorded as i32,
        AckOutcome::Conflict => v1::AckOutcome::Conflict as i32,
    }
}

/// Converts a claimed job to its wire representation.
///
/// # Errors
/// `internal` if the payload cannot be serialized to JSON (practically
/// unreachable: it is already a parsed `serde_json::Value`).
fn leased_job_to_proto(job: &koine_application::ports::LeasedJob) -> Result<v1::LeasedJob, Status> {
    let payload_json =
        serde_json::to_string(&job.payload).map_err(|_| Status::internal("payload encode"))?;
    Ok(v1::LeasedJob {
        job_id: job.job_id.to_string(),
        queue: job.queue.to_string(),
        payload_json,
        attempt: job.attempt,
        lease_id: job.lease.to_string(),
        expires_at_unix_ms: job.expires_at.timestamp_millis(),
        correlation_id: job.correlation_id.to_string(),
        traceparent: job.traceparent.clone(),
    })
}

#[tonic::async_trait]
impl<S, D, G, C, Sig, P> WorkerService for WorkerApi<S, D, G, C, Sig, P>
where
    S: EventStore + 'static,
    D: Dispatcher + 'static,
    G: IdGenerator + 'static,
    C: Clock + 'static,
    Sig: DispatchSignal + 'static,
    P: WorkerPresence + 'static,
{
    type FetchStream = ReceiverStream<Result<v1::LeasedJob, Status>>;

    async fn fetch(
        &self,
        request: Request<v1::FetchRequest>,
    ) -> Result<Response<Self::FetchStream>, Status> {
        let worker = auth::check(request.metadata(), &self.deps.config.token)?;
        let req = request.into_inner();
        let queue =
            QueueName::new(req.queue).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let ttl = clamp_lease_ttl(req.lease_ttl_ms, self.deps.config.max_lease_ttl)?;

        self.deps.presence.seen(&worker, Some(&queue)).await;

        let (tx, rx) = mpsc::channel(16);
        let deps = Arc::clone(&self.deps);
        tokio::spawn(async move {
            let lease_uc = LeaseNextJob {
                dispatcher: &deps.dispatcher,
            };
            loop {
                match lease_uc.execute(&queue, &worker, ttl).await {
                    Ok(Some(job)) => {
                        let message = match leased_job_to_proto(&job) {
                            Ok(message) => message,
                            Err(status) => {
                                let _ = tx.send(Err(status)).await;
                                break;
                            }
                        };
                        if tx.send(Ok(message)).await.is_err() {
                            // The worker disconnected between the claim and
                            // the send: the job is already leased and
                            // durably appended (ADR 0011), never merely
                            // held in memory. Its lease will simply expire
                            // and the sweep use case reclaims it for
                            // redelivery — crash-safety by design (ADR
                            // 0008); a dropped stream needs no special
                            // recovery path beyond ending this loop.
                            break;
                        }
                    }
                    Ok(None) => {
                        deps.signal.wait(&queue, deps.config.idle_poll).await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(Status::unavailable(format!("dispatch: {e}"))))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn start(
        &self,
        request: Request<v1::StartRequest>,
    ) -> Result<Response<v1::StartResponse>, Status> {
        let worker = auth::check(request.metadata(), &self.deps.config.token)?;
        let req = request.into_inner();
        let job_id = JobId::new(parse_uuid("job_id", &req.job_id)?);

        self.deps.presence.seen(&worker, None).await;

        let ack = WorkerAck {
            store: &self.deps.store,
            ids: &self.deps.ids,
            clock: &self.deps.clock,
        };
        ack.start(job_id, &worker)
            .await
            .map_err(|e| map_ack_error(&e))?;
        Ok(Response::new(v1::StartResponse {}))
    }

    async fn succeed(
        &self,
        request: Request<v1::SucceedRequest>,
    ) -> Result<Response<v1::AckResponse>, Status> {
        let worker = auth::check(request.metadata(), &self.deps.config.token)?;
        let req = request.into_inner();
        let job_id = JobId::new(parse_uuid("job_id", &req.job_id)?);
        let lease_id = LeaseId::new(parse_uuid("lease_id", &req.lease_id)?);
        let result = req
            .result_json
            .map(|raw| serde_json::from_str::<Value>(&raw))
            .transpose()
            .map_err(|_| Status::invalid_argument("malformed result_json"))?;

        let ack = WorkerAck {
            store: &self.deps.store,
            ids: &self.deps.ids,
            clock: &self.deps.clock,
        };
        let outcome = ack
            .succeed(job_id, &worker, lease_id, result)
            .await
            .map_err(|e| map_ack_error(&e))?;
        Ok(Response::new(v1::AckResponse {
            outcome: to_proto_outcome(outcome),
        }))
    }

    async fn fail(
        &self,
        request: Request<v1::FailRequest>,
    ) -> Result<Response<v1::AckResponse>, Status> {
        let worker = auth::check(request.metadata(), &self.deps.config.token)?;
        let req = request.into_inner();
        let job_id = JobId::new(parse_uuid("job_id", &req.job_id)?);
        let lease_id = LeaseId::new(parse_uuid("lease_id", &req.lease_id)?);
        let proto_error = req
            .error
            .ok_or_else(|| Status::invalid_argument("error is required"))?;
        let error = koine_domain::JobError {
            kind: proto_error.kind,
            message: proto_error.message,
            stacktrace: proto_error.stacktrace,
            retryable: proto_error.retryable,
        };

        let ack = WorkerAck {
            store: &self.deps.store,
            ids: &self.deps.ids,
            clock: &self.deps.clock,
        };
        let outcome = ack
            .fail(job_id, &worker, lease_id, error)
            .await
            .map_err(|e| map_ack_error(&e))?;
        Ok(Response::new(v1::AckResponse {
            outcome: to_proto_outcome(outcome),
        }))
    }

    async fn heartbeat(
        &self,
        request: Request<v1::HeartbeatRequest>,
    ) -> Result<Response<v1::HeartbeatResponse>, Status> {
        auth::check(request.metadata(), &self.deps.config.token)?;
        let req = request.into_inner();
        let lease_id = LeaseId::new(parse_uuid("lease_id", &req.lease_id)?);
        let ttl = Duration::from_millis(req.ttl_ms);

        let heartbeat = Heartbeat {
            dispatcher: &self.deps.dispatcher,
        };
        let alive = heartbeat
            .execute(lease_id, ttl)
            .await
            .map_err(|e| Status::internal(format!("dispatch: {e}")))?;
        Ok(Response::new(v1::HeartbeatResponse { alive }))
    }
}
