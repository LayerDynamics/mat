# Code blocks — syntax highlighting

Every fenced block is handed to `syntect` with a 24-bit truecolor escape stream. A dim italic language label sits above each block so the target language is obvious at a glance.

---

## Rust

```rust
use std::collections::HashMap;

#[derive(Debug)]
struct Greeter<'a> {
    greetings: HashMap<&'a str, &'a str>,
}

impl<'a> Greeter<'a> {
    fn new() -> Self {
        let mut g = HashMap::new();
        g.insert("en", "Hello");
        g.insert("ja", "こんにちは");
        g.insert("es", "Hola");
        Self { greetings: g }
    }

    fn greet(&self, lang: &str) -> String {
        match self.greetings.get(lang) {
            Some(word) => format!("{word}, world!"),
            None => String::from("Hello, world!"),
        }
    }
}
```

---

## TypeScript

```typescript
type Result<T, E = Error> =
  | { ok: true; value: T }
  | { ok: false; error: E };

async function fetchJson<T>(url: string): Promise<Result<T>> {
  try {
    const res = await fetch(url);
    if (!res.ok) {
      return { ok: false, error: new Error(`HTTP ${res.status}`) };
    }
    const value = (await res.json()) as T;
    return { ok: true, value };
  } catch (err) {
    return { ok: false, error: err as Error };
  }
}
```

---

## Python

```python
from dataclasses import dataclass
from typing import Iterator

@dataclass(frozen=True)
class Point:
    x: float
    y: float

    def distance_to(self, other: "Point") -> float:
        return ((self.x - other.x) ** 2 + (self.y - other.y) ** 2) ** 0.5


def walk(path: list[Point]) -> Iterator[float]:
    """Yield the distance travelled at each step along `path`."""
    for a, b in zip(path, path[1:]):
        yield a.distance_to(b)
```

---

## Go

```go
package main

import (
    "context"
    "fmt"
    "time"
)

func tick(ctx context.Context, interval time.Duration) <-chan time.Time {
    ch := make(chan time.Time)
    go func() {
        defer close(ch)
        t := time.NewTicker(interval)
        defer t.Stop()
        for {
            select {
            case now := <-t.C:
                ch <- now
            case <-ctx.Done():
                return
            }
        }
    }()
    return ch
}

func main() {
    ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
    defer cancel()
    for t := range tick(ctx, 500*time.Millisecond) {
        fmt.Println(t.Format(time.RFC3339))
    }
}
```

---

## Bash

```bash
#!/usr/bin/env bash
set -euo pipefail

install_mat() {
  local prefix="${PREFIX:-$HOME/.local}"
  local bin="$prefix/bin/mat"

  echo "→ installing mat to $bin"
  mkdir -p "$(dirname "$bin")"

  if command -v cargo >/dev/null 2>&1; then
    cargo install --root "$prefix" mat
  else
    echo "cargo not found; install Rust from https://rustup.rs first" >&2
    return 1
  fi
}

install_mat "$@"
```

---

## SQL

```sql
WITH recent_orders AS (
    SELECT
        customer_id,
        COUNT(*) AS n_orders,
        SUM(total_cents) / 100.0 AS total_usd
    FROM orders
    WHERE placed_at >= NOW() - INTERVAL '30 days'
    GROUP BY customer_id
)
SELECT c.name, r.n_orders, r.total_usd
FROM customers c
JOIN recent_orders r ON r.customer_id = c.id
WHERE r.total_usd > 100
ORDER BY r.total_usd DESC
LIMIT 25;
```

---

## JSON

```json
{
  "name": "mat",
  "version": "0.1.0",
  "description": "cat for rendered markdown",
  "bin": {
    "mat": "target/release/mat"
  },
  "features": ["syntax-highlighting", "inline-images", "osc8"],
  "platforms": ["linux", "macos", "windows"]
}
```

---

## Unknown language → plain-text fallback

```klingon
tlhIngan Hol Dajatlh'a'?
ghobe' — tlhIngan Hol vIjatlhlaHbe'.
```

Unknown lexers render with no per-token highlighting (uniform dim) but the language label is still printed so you can see what was requested.
