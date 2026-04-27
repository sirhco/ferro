#!/usr/bin/env bash
# End-to-end validation for the rich-text editor wiring.
# Prereq: `ferro-cli serve` running on :8080 with starter-blog data.
set -euo pipefail

API=${API:-http://127.0.0.1:8080}
EMAIL=${EMAIL:-me@example.com}
PASSWORD=${PASSWORD:-correct-horse-battery-staple}
SLUG=${SLUG:-validate-blocks-$(date +%s)}

step() { printf "\n\033[1;36m==> %s\033[0m\n" "$*"; }
ok()   { printf "    \033[1;32m✓\033[0m %s\n" "$*"; }
fail() { printf "    \033[1;31m✗\033[0m %s\n" "$*"; exit 1; }

step "1. Login"
TOKEN=$(curl -fsS -X POST "$API/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  -d "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}" \
  | sed -nE 's/.*"token":"([^"]+)".*/\1/p')
[ -n "$TOKEN" ] || fail "no token in login response"
ok "got token (${TOKEN:0:20}...)"

step "2. List published Pages (should include 'about', 'contact')"
LIST=$(curl -fsS "$API/api/v1/content/page?status=published" -H "Authorization: Bearer $TOKEN")
echo "$LIST" | grep -q '"about"'   && ok "found 'about'"   || fail "'about' missing"
echo "$LIST" | grep -q '"contact"' && ok "found 'contact'" || fail "'contact' missing"

step "3. Read 'about' — verify blocks shape on disk"
ABOUT=$(curl -fsS "$API/api/v1/content/page/about" -H "Authorization: Bearer $TOKEN")
echo "$ABOUT" | grep -q '"kind":"heading"'   && ok "has heading block"   || fail "no heading"
echo "$ABOUT" | grep -q '"kind":"paragraph"' && ok "has paragraph block" || fail "no paragraph"
echo "$ABOUT" | grep -q '"kind":"list"'      && ok "has list block"      || fail "no list"
echo "$ABOUT" | grep -q '"kind":"divider"'   && ok "has divider block"   || fail "no divider"

step "4. Create new Page with all 7 block kinds"
BODY=$(cat <<EOF
{
  "type_id": "01HQYTQ000000000000000000Z",
  "slug": "$SLUG",
  "locale": "en",
  "data": {
    "title": "Block Round Trip $SLUG",
    "slug": "$SLUG",
    "blocks": [
      { "kind": "heading", "level": 1, "text": "Round trip" },
      { "kind": "paragraph", "text": "Hello & <world>" },
      { "kind": "quote", "text": "Quoted", "cite": "Tester" },
      { "kind": "code", "lang": "rust", "code": "fn main(){}" },
      { "kind": "image", "media_id": "abc123", "alt": "alt" },
      { "kind": "list", "ordered": false, "items": ["a", "b"] },
      { "kind": "divider" }
    ]
  }
}
EOF
)
CREATE=$(curl -fsS -X POST "$API/api/v1/content/page" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d "$BODY")
echo "$CREATE" | grep -q "\"$SLUG\"" && ok "created $SLUG" || fail "create returned: $CREATE"

step "5. Read back — blocks survive round trip"
BACK=$(curl -fsS "$API/api/v1/content/page/$SLUG" -H "Authorization: Bearer $TOKEN")
for kind in heading paragraph quote code image list divider; do
  echo "$BACK" | grep -q "\"kind\":\"$kind\"" \
    && ok "$kind survived" || fail "$kind missing in round-trip"
done
echo "$BACK" | grep -q 'fn main(){}' && ok "code body preserved" || fail "code body lost"

step "6. Render preview HTML"
PREVIEW=$(curl -fsS "$API/preview/page/$SLUG" -H "Authorization: Bearer $TOKEN")
echo "$PREVIEW" | grep -q '<h1>Round trip</h1>' && ok "h1 rendered"      || fail "h1 missing"
echo "$PREVIEW" | grep -q '&lt;world&gt;'        && ok "html escaped"    || fail "escape failed"
echo "$PREVIEW" | grep -q '<blockquote><p>Quoted'&& ok "quote rendered"  || fail "quote missing"
echo "$PREVIEW" | grep -q 'class="language-rust"'&& ok "code lang class" || fail "code class missing"
echo "$PREVIEW" | grep -q '<ul><li>a</li>'       && ok "list rendered"   || fail "list missing"
echo "$PREVIEW" | grep -q '<hr />'               && ok "divider rendered"|| fail "divider missing"

step "7. Update body — Markdown field (existing Post)"
PATCH=$(curl -fsS -X PATCH "$API/api/v1/content/post/hello-ferro" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"data":{"title":"Hello, Ferro","slug":"hello-ferro","excerpt":"upd","body":"# H1\n\n**bold** and `code`\n\n| a | b |\n|---|---|\n| 1 | 2 |\n","tags":["intro"],"author_id":"01HQYAJ000000000000000000Z"}}')
ok "patched hello-ferro"
PMD=$(curl -fsS "$API/preview/post/hello-ferro" -H "Authorization: Bearer $TOKEN")
echo "$PMD" | grep -q '<h1>H1</h1>'        && ok "markdown h1"       || fail "markdown h1 missing"
echo "$PMD" | grep -q '<strong>bold</strong>' && ok "markdown bold"  || fail "markdown bold missing"
echo "$PMD" | grep -q '<table>'            && ok "markdown table"    || fail "markdown table missing"

step "8. Cleanup"
curl -fsS -X DELETE "$API/api/v1/content/page/$SLUG" \
  -H "Authorization: Bearer $TOKEN" >/dev/null && ok "deleted $SLUG" || true

printf "\n\033[1;32mAll rich-text editor checks passed.\033[0m\n"
