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

## Social covers

`site/media/social/keyala-cover-a-<language>-v1.png` contains the approved
1200×630 A layout in all three languages. The shared page/news metadata selects
the page language and includes image type, dimensions and translated alt text.
Change the filename version when replacing an image to avoid reusing an old URL.

The editable layout is `tools/site/social-cover.html`. To render it, copy it to a
local preview folder together with `site/fonts/` and `site/brand/`, serve that
folder, and open `/social-cover.html?lang=en` (or `zh-Hant`, `ja`) at a 1200×630
viewport. Wait for fonts to load, then export the full canvas as PNG. The exported
PNGs are committed assets; normal site builds do not need a browser. Keep all
critical content within the 80px inset and review a half-size preview.

Search favicon assets are rasterized from `site/brand/koala.svg`: a 96px PNG,
48px root ICO and 180px Apple touch icon. The generated head keeps the SVG for
browsers and declares all raster URLs. Only the domain homepage emits `WebSite`
JSON-LD with name `Keyala` and URL `https://keyala.io/`; locale subdirectories do
not declare separate sites. Google controls whether and when these appear.

The staging-only Plaza reuses `site/examples/index.html`. After changing that page,
`site/base.css`, `site/i18n.js` or `site/examples/plaza.js`, run
`node tools/site/build-plaza.mjs` to refresh the backend's embedded HTML and JS.
`node tools/site/build-plaza.mjs --check` detects stale copies in backend CI.
The generated page embeds its styling and logo so the backend's strict CSP stays intact.
