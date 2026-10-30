#!/bin/bash
# Vercel's Ignored Build Step (ignoreCommand in vercel.json): exit 0 skips the
# build, anything else builds it. Run from site/, the project's Root Directory.
#
# Production, which is main and the release's deploy hook, always builds. A
# preview builds when something the site reads changed since its branch's last
# deployment: the Site workflow's paths, without site.yml, which Vercel doesn't
# use. A branch with no deployment yet, or whose last one isn't in Vercel's
# shallow clone (after a force push), builds. So does any git error.
set -u

[[ ${VERCEL_ENV:-} == production ]] && exit 1
previous=${VERCEL_GIT_PREVIOUS_SHA:-}
[[ -n $previous ]] && git cat-file -e "$previous^{commit}" 2>/dev/null || exit 1
cd "$(git rev-parse --show-toplevel)" || exit 1
echo "Changed since $previous:"
git diff --name-only "$previous" HEAD
git diff --quiet "$previous" HEAD -- site docs .github/assets apps/desktop/src/styles/tokens.css
