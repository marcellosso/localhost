Select a Service to see its output, with colours. **All services** shows every Service of the Project in one stream, each line prefixed with its Key. **⌘F** filters, **⌃L** (Ctrl+L) clears the view; log files are kept. URLs in the output are clickable.

![One Service selected in light mode: Web is Running on port 5173, with Open, Restart and Stop buttons and its Vite output](../../../../.github/assets/service-light.png)

Output goes through pipes, not a terminal, so localhost sets `FORCE_COLOR=1`, `CLICOLOR_FORCE=1` and `PYTHONUNBUFFERED=1` for every Service: tools keep their colours and don't hold output back.

## What is kept

Each Run keeps its latest 10,000 lines in memory for the window, and writes everything to a log file. The last 5 Runs of each Service are kept on disk, up to 10 MB each. See [Data and storage](/docs/data) for where.
