# MCP interface

Enable the service in **Settings → MCP interface**. Configure your client for **Streamable HTTP**, use the displayed `/mcp` address, and add:

```text
Authorization: Bearer <access-token>
```

The service is disabled by default. Once enabled, it listens on `127.0.0.1` and defaults to read-only access. **Allow organization and imports** permits creating logical folders, editing tags, changing folder membership, and referencing local files. No plugin installation or download is required.

Choose **This computer only** or **Computer and LAN** for access scope. LAN access still requires the token and validates remote addresses, Host, Origin, and request rate. Rotating the token immediately revokes the old one; disabling the service stops its listener. The token stays in this application's configuration directory with Unix permissions 0600.

## Tools

| Tool                    | Purpose                                                                          |
| ----------------------- | -------------------------------------------------------------------------------- |
| `list_libraries`        | List registered libraries                                                        |
| `get_library_context`   | Get a library's folders and tags                                                 |
| `search_assets`         | Semantic text search, filenames, and metadata filters                            |
| `get_asset`             | Asset details and cover resource URI                                             |
| `get_asset_metadata`    | Media metadata for a registered asset                                            |
| `create_folder`         | Create a logical folder; requires write access                                   |
| `update_asset_tags`     | Add/remove tags; requires write access                                           |
| `move_assets_to_folder` | Change logical folder membership; requires write access                          |
| `import_assets`         | Submit absolute local paths to the reference import queue; requires write access |
| `get_operation`         | Read persistent task progress by library and operationId                         |

Pass `libraryId` to address a library explicitly without changing desktop selection. Semantic search shares the same EmbeddingGemma 2 / Qdrant index. Video results represent whole assets without segment positions.

Imports retain reference-only behavior and the 32-second video limit. The operationId is also the desktop batch ID, and its work appears in the desktop queue. Progress summaries count processing tasks, including imports and requested indexes.

Cover resources use `clipstudio-open://libraries/{libraryId}/assets/{assetUid}/preview`. Only images for registered, live assets under library-owned derived storage can be read. There are no online-download, trimming, subtitle, permanent-deletion, or arbitrary-file-read tools.

See [testing](testing.md) for HTTP, authentication, and real-model checks.
