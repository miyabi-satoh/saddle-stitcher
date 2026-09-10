# CLAUDE.md

## プロジェクトルール

- ブランチモデルは GitHub Flow
- スカッシュマージ
- 対話型コマンドには手を出さない。素直にユーザに譲ること。
- main にマージされるコードは Codex レビュー通過済みであること。
  - レビューおよび CI を通過した PR は main にマージする
- Codex レビューは `codex:codex-rescue` サブエージェント (Agent tool) に依頼する。専用のペインは要らない。
  このサブエージェントは依頼文を整えて Codex へ渡すところまでしかやらない。リポジトリを調べて対象を選んだり、観点を補ったりはしない。依頼文に以下を自分で書くこと。
  - レビュー対象 (ブランチなら `git diff main...HEAD` のように比較基準まで書く。作業ツリーならその旨)
  - 見てほしい観点
  - 読み取り専用であること・ファイルを編集しないこと (レビュー依頼と判断されれば書き込みなしで走るが、既定は書き込み可なので明示しておく)
  - 指摘は actionable なものだけ、なければ「指摘なし」とだけ返すこと
  指摘の反映は Claude 側の仕事。
  なお `/codex:review` と `/codex:adversarial-review` は `disable-model-invocation` のため Claude からは起動できない。ユーザーが自分で打つとき用。
- `.svelte` ファイルの編集は svelte-file-editor エージェント / svelte-autofixer を通すこと

## このアプリについて

A4 PDF を A3 中綴じ見開き PDF に変換するツール。[rustvelte](https://github.com/miyabi-satoh/rustvelte)
のscaffoldで生成し、[Tauri-NextTS-SaddleStitcher](https://github.com/miyabi-satoh/Tauri-NextTS-SaddleStitcher)
(Tauri+Next.js+Python) をリファクタしたもの。詳細は README.ja.md 参照。

- PDF変換ロジック本体: `src/pdf/saddle_stitch.rs` (lopdf)
- APIハンドラ: `src/api/pdf.rs`
- フロントエンド: `frontend/src/routes/+page.svelte` (1画面のみ)
