# DNA Calc website preview

Build the content site from pushed master with the existing site tool.

```sh
cargo xtask site pack content --include-drafts
cargo xtask site serve content --port 8080
cargo xtask site verify content --url http://127.0.0.1:8080/
```

Run the serve and verify commands in separate terminals. Serve reads the same header snippet as
Caddy. Add `--release target/site/content/<release>` to serve or verify when reviewing a specific
pack. The packer hashes the styles, scripts and both application bundles under `assets/<id>/`.
The build manifest records the source revision and every output file. The archive containing
unapproved blog drafts is for local review.

The content pack includes `/llms.txt` and Markdown editions of public Markdown-backed guides,
articles and posts. The HTML pages advertise available editions. Verification checks the index,
Markdown links and HTML metadata against the served manifest. The index uses the content host
from the site tool, while preview verification resolves its local links on the preview server.

The same-origin Windows-oriented site is at `/`. The full web app is available at `/app/`
and Formula Explorer is mounted on Home, Playground and its product page. To change the public
app link, edit `browser_app_url` in `layout.json`. Use `https://app.dnacalc.com/` for a
separate app host, `/app/` on the same host, or `https://dnacalc.com/` if the app stays at the
apex and the content is hosted elsewhere. The separate web app pack is still
`cargo xtask site pack calc`. The layout setting controls links, while the owner controls the
host routing.

Edit `pages/home.html` to change the lead audience and `pages/about.html` to update the About
page. Add blog posts through the Markdown pipeline in `blog/`. Home and Status read the oracle
baseline in the release plan and the full-bank rows in `witness-status.tsv`. The download
preview uses the first-preview release draft and the package tool's Windows filename rules.
The sample workbook is the repository's generated CalcDoc D1 fixture.

The Playground and Home cards come from `docs/fun/index.html`. The standalone exhibits use local
styles, scripts and system fonts. The site includes the vendored D3 licence in its third-party
notices. Refresh the bank with `python3 docs/fun/tools/refresh_function_bank.py`.
