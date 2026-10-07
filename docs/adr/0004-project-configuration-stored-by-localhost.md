# Project Configuration is stored by localhost, not in the project folder

Project Configuration lives in localhost's own storage under `~/Library/Application Support/localhost/` and the app never writes into a user's project folder. An in-repo `localhost.yml` (committable, shareable) was considered and rejected for V1: writing into users' repos is a surprising side effect and sharing is an explicit later direction. Export/import (M3) is the bridge if an in-repo format is wanted later.
