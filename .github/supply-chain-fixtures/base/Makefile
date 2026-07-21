TLA_TOOLS := docs/formal/.tools/tla2tools.jar
TLA_TOOLS_VERSION := 1.7.4
TLA_TOOLS_SHA256 := 936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
TLA_TOOLS_URL := https://github.com/tlaplus/tlaplus/releases/download/v$(TLA_TOOLS_VERSION)/tla2tools.jar

md:
	npm ci --ignore-scripts
	npm exec -- markdownlint-cli2 "**/*.md"

$(TLA_TOOLS):
	mkdir -p docs/formal/.tools
	curl -fsSL $(TLA_TOOLS_URL) -o $(TLA_TOOLS).tmp
	echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS).tmp" | sha256sum -c
	mv $(TLA_TOOLS).tmp $(TLA_TOOLS)

tla: $(TLA_TOOLS)
	echo "$(TLA_TOOLS_SHA256)  $(TLA_TOOLS)" | sha256sum -c
	cd docs/formal && java -jar .tools/tla2tools.jar model.tla
