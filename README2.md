# Fetchless — Guia de Uso

Proxy web que converte páginas HTML/JSON em texto limpo antes de entrar no contexto de um agente de IA. Redução típica de **86–99% de tokens**.

---

## Pré-requisitos

- [Rust](https://rustup.rs/) (stable, 1.75+)
- Conexão com a internet (para buscar URLs)

---

## Instalação

```bash
git clone <repo>
cd web-fetch-low-tokens

# Build de desenvolvimento
cargo build

# Build otimizado (recomendado para uso real)
cargo build --release
```

O binário compilado fica em `target/release/fetchless` (ou `target/debug/fetchless`).

---

## Modos de uso

O fetchless tem dois modos: **servidor HTTP REST** e **servidor MCP** (para integração com agentes via stdio).

### Modo HTTP (padrão)

```bash
# Inicia na porta 8080, bind em 127.0.0.1
cargo run -- --port 8080

# Ou com o binário compilado
./target/release/fetchless --port 8080
```

Opções disponíveis:

| Flag | Padrão | Descrição |
|------|--------|-----------|
| `--port` | `8080` | Porta HTTP |
| `--bind` | `127.0.0.1` | Endereço de bind (use `0.0.0.0` para expor na rede) |
| `--db-path` | `agent_proxy.db` | Caminho do banco SQLite |
| `--default-ttl` | `300` | TTL padrão do cache em segundos |
| `--mcp` | — | Ativa modo MCP em vez de HTTP |

### Modo MCP

Para usar como ferramenta MCP num agente de IA (ex: Claude Desktop):

```bash
cargo run -- --mcp
```

Ferramentas expostas: `fetch_clean`, `fetch_clean_batch`, `refine_prompt`.

---

## Endpoints REST

### `GET /`

Retorna informações do servidor e estatísticas gerais.

```bash
curl http://localhost:8080/
```

```json
{
  "name": "Fetchless",
  "version": "0.1.0",
  "layers": { "1": "Prompt Refiner (opt-in)", "2": "Data Proxy (active)" },
  "endpoints": { ... },
  "stats": { ... }
}
```

---

### `POST /fetch` — Buscar e limpar uma URL

Busca uma página web e retorna apenas o conteúdo relevante em texto limpo.

**Requisição:**
```bash
curl -X POST http://localhost:8080/fetch \
  -H "Content-Type: application/json" \
  -d '{"url": "https://en.wikipedia.org/wiki/Rust_(programming_language)"}'
```

**Com TTL customizado (cache em segundos):**
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

> **Nota:** Somente URLs HTTPS são aceitas. HTTP é rejeitado. IPs privados (RFC 1918, loopback, etc.) são bloqueados por segurança.

---

### `POST /fetch/batch` — Buscar múltiplas URLs

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

> Limite de 10 URLs por requisição batch.

---

### `POST /refine` — Refinar prompt (Layer 1)

Remove palavras de preenchimento de um texto, preservando entidades importantes (tickers, valores monetários, negações, referências temporais).

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

> O campo `suggested` é uma sugestão. Você decide se usa o texto original ou o refinado.

---

### `GET /stats` — Estatísticas de uso

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

O fetchless usa SQLite para cachear respostas. Se você buscar a mesma URL duas vezes dentro do TTL, a segunda requisição retorna `"from_cache": true` instantaneamente.

```bash
# TTL padrão: 300 segundos (5 minutos)
# Para desabilitar cache numa requisição, use ttl: 0
curl -X POST http://localhost:8080/fetch \
  -d '{"url": "https://example.com/news", "ttl": 0}'
```

O banco de dados é salvo em `agent_proxy.db` por padrão. Para usar outro caminho:

```bash
cargo run -- --db-path /tmp/meu-cache.db
```

---

## Exemplos práticos

### Usar com Python (requests)

```python
import requests

resp = requests.post("http://localhost:8080/fetch", json={
    "url": "https://docs.python.org/3/library/asyncio.html"
})
data = resp.json()
print(f"Redução: {data['reduction_pct']}%")
print(data['content'][:500])
```

### Usar com Node.js (fetch)

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

# O agente chama fetch_clean("https://...") e recebe só o texto relevante
```

---

## Testes

```bash
# Testes offline (unitários + integração)
cargo test

# Incluir testes com URLs reais (requer internet)
cargo test -- --include-ignored

# Lint
cargo clippy -- -D warnings

# Verificar formatação
cargo fmt --check
```

---

## Limitações e segurança

- **Somente HTTPS** — requisições HTTP são rejeitadas
- **IPs públicos apenas** — bloqueia RFC 1918 (192.168.x.x, 10.x.x.x, 172.16–31.x.x), loopback (127.x.x.x), link-local, CGNAT
- **Batch limitado a 10 URLs** por requisição
- **Bind local por padrão** — o servidor escuta apenas em `127.0.0.1`; para expor na rede use `--bind 0.0.0.0` (com cautela)
- O servidor não faz autenticação — não exponha publicamente sem um proxy reverso com auth
