# metalcraft-agent-gateway

Platform-agnostic messaging gateway for [metalcraft-agent](https://github.com/rust4ai/metalcraft-agent).

Exposes a single HTTP API that translates into platform-specific calls (Discord, Slack, etc.).
metalcraft-agent talks to the gateway — it never needs to know which platform is behind it.

## Protocol

```
Base: $AGENT_GATEWAY_URL/api/v1
Auth: Authorization: Bearer $AGENT_GATEWAY_API_KEY

POST   /messages                           {channel_id, content, message_reference_id?}
PATCH  /messages/{message_id}              {channel_id, content}
PUT    /messages/{message_id}/reactions     {channel_id, emoji}
GET    /channels/{channel_id}/messages?limit=N
GET    /channels/{channel_id}
```

## Supported platforms

| Platform | Env vars needed |
|----------|----------------|
| Discord  | `DISCORD_BOT_TOKEN` |
| Slack    | `SLACK_BOT_TOKEN` |

Set `PLATFORM=discord` or `PLATFORM=slack` to choose.

## Running locally

```bash
cp .env.example .env   # fill in your tokens
cargo run
```

## Docker

```bash
docker build -t metalcraft-agent-gateway .
docker run --env-file .env -p 3000:3000 metalcraft-agent-gateway
```

## Deploy to Railway

1. Push this repo to GitHub
2. Create a new Railway project → "Deploy from GitHub repo"
3. Add environment variables (`PLATFORM`, `DISCORD_BOT_TOKEN` or `SLACK_BOT_TOKEN`, `AGENT_GATEWAY_API_KEY`)
4. Railway auto-detects the Dockerfile via `railway.toml`

## License

MIT
