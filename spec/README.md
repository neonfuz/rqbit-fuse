# Torrent-Fuse Specification

This directory contains the detailed technical specifications for the torrent-fuse project.

## Documents

### 📋 Architecture (`architecture.md`)
High-level system architecture and design overview.

**Contents:**
- System architecture diagram
- Component responsibilities (FUSE client, rqbit server)
- Directory structure layout
- FUSE implementation details (inode management, callbacks)
- Configuration schema
- CLI interface specification
- Security and performance considerations

**Read this first** to understand the overall system design.

---

### 🔌 API Documentation (`api.md`)
Detailed documentation of the rqbit HTTP API and how torrent-fuse interacts with it.

**Contents:**
- All rqbit API endpoints used
- Request/response examples
- HTTP Range request strategy
- Piece availability bitfield handling
- Error responses and rate limiting
- FUSE read flow diagram

**Reference this** when implementing the API client or debugging HTTP interactions.

---

### 🛠️ Technical Design (`technical-design.md`)
Low-level implementation details and data structures.

**Contents:**
- Data structures (Torrent, TorrentFile, InodeEntry, FileAttr)
- Inode management with concurrent access (DashMap)
- FUSE callback implementations
- HTTP read with retry logic and backoff
- Cache implementation with TTL
- Error mapping to FUSE error codes
- Configuration structures

**Use this** when writing the actual implementation code.

---

### 🗺️ Roadmap (`roadmap.md`)
Development phases and timeline.

**Contents:**
- 6 development phases (Week 1-8)
- Detailed task breakdown per phase
- Testing strategy
- Directory structure
- Dependencies list
- Release checklist

**Follow this** to track development progress.

---

### 🚀 Quick Start (`quickstart.md`)
User-facing documentation for installing and using torrent-fuse.

**Contents:**
- Installation prerequisites
- Step-by-step usage guide
- Command reference
- Configuration examples
- Troubleshooting guide
- Performance tips

**Share this** with users who want to use torrent-fuse.

---

### ✅ Ralph (`ralph.md`)
Guidelines for writing effective todo tasks and structuring project work.

**Contents:**
- Task writing best practices
- How to structure complex tasks
- Breaking down work into manageable steps
- Quality standards for task documentation

**Reference this** when writing todo tasks to ensure consistency and completeness.

## Reading Order

1. **For understanding the project:**
   1. `architecture.md` - Get the big picture
   2. `api.md` - Understand the API interactions
   3. `quickstart.md` - See how users will interact with it

2. **For implementation:**
   1. `architecture.md` - Understand the design
   2. `technical-design.md` - See the implementation details
   3. `api.md` - Reference during API client development
   4. `roadmap.md` - Follow the development plan

3. **For users:**
   - Start with `quickstart.md`

## File Status

All specifications are complete and ready for implementation.

Last updated: 2024-02-13
