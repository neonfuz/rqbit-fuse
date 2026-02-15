# Reference Documentation

This directory contains external reference documentation and third-party specifications.

**Important:** These files are for reference only and should not be modified. They represent external resources, API specifications, and documentation from outside sources.

## Contents

- **streaming-api.md** - rqbit HTTP streaming API specification
  - Streaming URLs and endpoints
  - HTTP Range request support for seeking
  - Response headers and status codes
  - Playlist API (M3U8)
  - Implementation details and examples

- **ralph.md** - Ralph AI workflow playbook
  - Complete guide to the Ralph AI development methodology
  - Loop mechanics and control flow
  - Prompt templates and file organization
  - Best practices for AI-assisted development

## Usage

Reference these documents when implementing features that integrate with external APIs or when following external development methodologies. For any project-specific documentation or modifications, create files in the `spec/` directory instead.

---

**Note:** If you find that reference documentation is outdated or incorrect, do not modify these files directly. Instead:
1. Create a new file in `spec/` with the updated information
2. Document the discrepancy in the new file
3. Reference the external source and note the version/date of the original documentation
