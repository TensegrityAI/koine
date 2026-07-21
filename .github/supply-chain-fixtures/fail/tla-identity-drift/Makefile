TLA_TOOLS := docs/formal/.tools/tla2tools.jar
TLA_TOOLS_VERSION := 1.7.5
TLA_TOOLS_SHA256 := aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
TLA_TOOLS_URL := https://github.com/tlaplus/tlaplus/releases/download/v$(TLA_TOOLS_VERSION)/tla2tools.jar

md:
	npm ci --ignore-scripts
	npm exec -- markdownlint-cli2 "**/*.md" "!_archive" "!target" "!node_modules" "!docs/superpowers" "!.superpowers"

supply-chain:
	npm ci --ignore-scripts
	bash .github/scripts/check-supply-chain.sh
	bash .github/scripts/test-supply-chain.sh

$(TLA_TOOLS):
	mkdir -p docs/formal/.tools
	curl -fsSL $(TLA_TOOLS_URL) -o $(TLA_TOOLS).tmp
	echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS).tmp" | sha256sum -c
	mv $(TLA_TOOLS).tmp $(TLA_TOOLS)

tla: $(TLA_TOOLS)
	echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS)" | sha256sum -c
	cd docs/formal && java -XX:+UseParallelGC -jar .tools/tla2tools.jar -config lease_protocol.cfg lease_protocol.tla
