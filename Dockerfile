# For MCP directories' introspection only, such as Glama's: it runs
# `localhost mcp` so they can list its tools. Inside a container it sees no
# Projects and can't start or stop anything on your computer: for real use,
# install the app (https://www.getlocalhost.app) and connect your agents with
# `npx -y @getlocalhost/cli mcp install`.
#
# The `localhost` command from release VERSION's .deb; VERSION=latest takes the latest release:
#   docker build --platform=linux/amd64 -t localhost-mcp .
#   docker build --platform=linux/amd64 --build-arg VERSION=latest -t localhost-mcp .
# The Linux app is amd64 only.
#
# Bump VERSION with each release, so directories list that release's tools.
FROM debian:bookworm-slim

ARG VERSION=0.4.0
RUN set -eu; \
    apt-get update; \
    apt-get install -y --no-install-recommends curl ca-certificates; \
    if [ "$VERSION" = latest ]; then \
      VERSION=$(curl -fsSLI -o /dev/null -w '%{url_effective}' https://github.com/marcellosso/localhost/releases/latest | sed 's|.*/tag/v||'); \
    fi; \
    curl -fsSL -o /tmp/localhost.deb "https://github.com/marcellosso/localhost/releases/download/v${VERSION}/localhost-app_${VERSION}_amd64.deb"; \
    dpkg-deb -x /tmp/localhost.deb /tmp/localhost; \
    install -m 0755 /tmp/localhost/usr/bin/localhost /usr/local/bin/localhost; \
    localhost --version; \
    apt-get purge -y --auto-remove curl; \
    rm -rf /tmp/localhost /tmp/localhost.deb /var/lib/apt/lists/*

ENTRYPOINT ["localhost", "mcp"]
