***CRITICAL*** Read this file, do the first item on the checklist, and then edit the checklist file to check the item off the list. Do not ask if you should do it, just do it.
***CRITICAL*** After doing an item, write a git commit message to the .git/COMMIT_EDITMSG file about what you did
***CRITICAL*** After each step, if you think it requires more work then add more todo items to the end of the list
***CRITICAL*** When you do research, write your findings into a new file in the 'research' subdirectory and make a reference to it in the checklist after checking the item off the list
***CRITICAL*** If you are done with every item in the checklist, create an empty file in the root directory named .done

## Current Task Checklist

Task: Implement rqbit HTTP API client (Phase 1)

- [x] Read spec/api.md to understand rqbit API
- [x] Read existing codebase to understand current structure
- [x] Create api/mod.rs with RqbitClient struct
- [x] Implement POST /torrents - add torrent
- [x] Implement GET /torrents - list torrents
- [x] Implement GET /torrents/{id} - get details
- [x] Implement GET /torrents/{id}/files - list files (via TorrentInfo)
- [x] Implement GET /torrents/{id}/pieces - get availability
- [x] Implement GET /torrents/{id}/read with Range support
- [x] Add retry logic with exponential backoff
- [x] Map API errors to appropriate types
- [x] Write tests for API client
- [x] Run cargo test, clippy, and fmt
