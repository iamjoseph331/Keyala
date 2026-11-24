# Static multilingual site

English uses `/`, Traditional Chinese `/zh-Hant/`, and Japanese `/ja/`.
News articles keep `/news/<slug>/<language>/`. Source HTML contains the existing
translation dictionaries; the build emits one language per page. `site/seo.json`
owns page metadata; homepage copy stays in its HTML translation dictionary. The URL fixes the language, with native
HTML links for language selection. No browser language detection runs in output.

Build a preview outside the source tree:

```sh
npm ci --prefix tools/site --ignore-scripts --no-audit --no-fund
mkdir -p /tmp/keyala-preview
cp -R site/. /tmp/keyala-preview/
python3 tools/build_news.py /tmp/keyala-preview
node tools/site/build.mjs /tmp/keyala-preview
npm test --prefix tools/site
python3 -m http.server 8772 --directory /tmp/keyala-preview
```

The Python news builder runs first; the locale build owns the final sitemap.
GitHub Pages runs the same pipeline. Do not serve source templates as the final
site or edit generated locale pages. A fresh output folder prevents stale posts.

Developer docs live in `site/docs/keyala.md`, `keyala.zh-Hant.md` and
`keyala.ja.md`. Translate prose together, preserving code blocks, inline API
identifiers and heading order. Tests enforce these invariants. The build uses
the existing Marked version to render complete HTML, tables and stable section
anchors. Client JavaScript only adds copying, scroll highlighting, mobile table
of contents behavior and News category filters.

Privacy and support pages retain `noindex` and stay out of the sitemap.
The legacy operational console, review tool and tutorial redirect are not locale
content pages. A metadata change does not establish Google indexing or ranking;
use Search Console to evaluate that separately.
