# Fetchless

Proxy web que converte páginas HTML/JSON em texto limpo antes de entrar no contexto de um agente de IA. Redução típica de **86–99% de tokens**.

No API key. No LLM. No GPU. Single binary.

| Fonte | Tokens brutos | Após proxy | Redução |
|-------|--------------|------------|---------|
| Yahoo Finance (AAPL) | 704.760 | 2.625 | **99,6%** |
| Wikipedia (artigo) | 154.440 | 19.479 | **87,4%** |
| Hacker News | 8.662 | 859 | **90,1%** |

---

## Instalação

Requer [Rust](https://rustup.rs/) 1.75+.

```bash
git clone <repo>
cd web-fetch-low-tokens

# Build otimizado
cargo build --release
```

O binário fica em `target/release/fetchless`.

---

## Modos de uso

O fetchless tem dois modos: **servidor HTTP REST** e **servidor MCP** (integração com agentes via stdio).

### Modo HTTP (padrão)

```bash
./target/release/fetchless --port 8080
```

| Flag | Padrão | Descrição |
|------|--------|-----------|
| `--port` | `8080` | Porta HTTP |
| `--bind` | `127.0.0.1` | Endereço de bind |
| `--db-path` | `agent_proxy.db` | Caminho do banco SQLite |
| `--default-ttl` | `300` | TTL padrão do cache (segundos) |
| `--mcp` | — | Ativa modo MCP |

### Modo MCP

Para usar como ferramenta MCP num agente de IA (ex: Claude Desktop):

```bash
./target/release/fetchless --mcp
```

Ferramentas expostas: `fetch_clean`, `fetch_clean_batch`, `refine_prompt`.

---

## Endpoints REST

### `POST /fetch` — Buscar e limpar uma URL

```bash
curl -X POST http://localhost:8080/fetch \
  -H "Content-Type: application/json" \
  -d '{"url": "https://en.wikipedia.org/wiki/Rust_(programming_language)"}'
```

**Com TTL customizado:**
```bash
curl -X POST http://localhost:8080/fetch \
  -H "Content-Type: application/json" \
  -d '{"url": "https://docs.rs/tokio/latest/tokio/", "ttl": 600}'
```

**Resposta:**
```json
{
  "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)",
  "content": "Rust is a multi-paradigm, general-purpose programming language...",
  "content_type": "html",
  "original_tokens": 12500,
  "cleaned_tokens": 980,
  "reduction_pct": 92.1,
  "from_cache": false
}
```

### `POST /fetch/batch` — Múltiplas URLs

```bash
curl -X POST http://localhost:8080/fetch/batch \
  -H "Content-Type: application/json" \
  -d '{
    "urls": [
      "https://doc.rust-lang.org/book/ch01-00-getting-started.html",
      "https://doc.rust-lang.org/book/ch02-00-guessing-game-tutorial.html"
    ],
    "ttl": 300
  }'
```

**Resposta:**
```json
{
  "results": [
    { "url": "...", "content": "...", "original_tokens": 8000, "cleaned_tokens": 650, "from_cache": false },
    { "url": "...", "content": "...", "original_tokens": 9200, "cleaned_tokens": 720, "from_cache": false }
  ],
  "total_original_tokens": 17200,
  "total_cleaned_tokens": 1370,
  "total_reduction_pct": 92.0
}
```

> Limite de 10 URLs por requisição.

### `POST /refine` — Refinar prompt (opcional)

Remove palavras de preenchimento preservando entidades importantes (tickers, valores monetários, negações, referências temporais).

```bash
curl -X POST http://localhost:8080/refine \
  -H "Content-Type: application/json" \
  -d '{"text": "Basically, I just wanted to kind of ask you about maybe potentially using Rust for our new project, if that would be okay."}'
```

**Resposta:**
```json
{
  "original": "Basically, I just wanted to kind of ask you about maybe potentially using Rust...",
  "suggested": "I wanted to ask about using Rust for our new project.",
  "original_tokens": 32,
  "suggested_tokens": 11,
  "savings_pct": 65.6,
  "confidence": 0.92,
  "protected_entities": []
}
```

> `suggested` é uma sugestão — você decide se usa o texto original ou o refinado.

### `GET /stats` — Estatísticas

```bash
curl http://localhost:8080/stats
```

```json
{
  "layer1_requests": 15,
  "layer1_tokens_saved": 340,
  "layer2_requests": 42,
  "layer2_tokens_saved": 98450,
  "total_tokens_saved": 98790,
  "est_cost_saved": 0.148
}
```

---

## Cache

O fetchless usa SQLite para cachear respostas. A segunda requisição para a mesma URL dentro do TTL retorna `"from_cache": true` instantaneamente.

```bash
# Desabilitar cache numa requisição específica
curl -X POST http://localhost:8080/fetch \
  -d '{"url": "https://example.com/news", "ttl": 0}'

# Usar caminho customizado para o banco
./target/release/fetchless --db-path /tmp/meu-cache.db
```

---

## Exemplos práticos

### Python

```python
import requests

resp = requests.post("http://localhost:8080/fetch", json={
    "url": "https://docs.python.org/3/library/asyncio.html"
})
data = resp.json()
print(f"Redução: {data['reduction_pct']}%")
print(data['content'][:500])
```

### Node.js

```js
const res = await fetch("http://localhost:8080/fetch", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ url: "https://nodejs.org/en/docs" }),
});
const data = await res.json();
console.log(`${data.original_tokens} → ${data.cleaned_tokens} tokens`);
```

### Integrar num agente de IA

```python
import requests

def fetch_clean(url: str) -> str:
    """Ferramenta para agente: busca URL e retorna conteúdo limpo."""
    resp = requests.post("http://localhost:8080/fetch", json={"url": url})
    resp.raise_for_status()
    return resp.json()["content"]
```

---

## Segurança e limitações

- **Somente HTTPS** — requisições HTTP são rejeitadas
- **IPs públicos apenas** — bloqueia RFC 1918, loopback, link-local, CGNAT
- **Batch limitado a 10 URLs** por requisição
- **Bind local por padrão** — escuta apenas em `127.0.0.1`; use `--bind 0.0.0.0` para expor na rede (com cautela)
- Sem autenticação — não exponha publicamente sem um proxy reverso com auth

---

## License

MIT
