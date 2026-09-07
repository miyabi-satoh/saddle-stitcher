# CLAUDE.md

## プロジェクトルール

- ブランチモデルは GitHub Flow
- スカッシュマージ
- 対話型コマンドには手を出さない。素直にユーザに譲ること。
- main にマージされるコードはセカンドオピニオンのレビュー通過済みであること。
  - レビューおよび CI を通過した PR は main にマージする
- `.svelte` ファイルの編集は svelte-file-editor エージェント / svelte-autofixer を通すこと

## このアプリについて

A4 PDF を A3 中綴じ見開き PDF に変換するツール。[rustvelte](https://github.com/miyabi-satoh/rustvelte)
のscaffoldで生成し、[Tauri-NextTS-SaddleStitcher](https://github.com/miyabi-satoh/Tauri-NextTS-SaddleStitcher)
(Tauri+Next.js+Python) をリファクタしたもの。詳細は README.ja.md 参照。

- PDF変換ロジック本体: `src/pdf/saddle_stitch.rs` (lopdf)
- APIハンドラ: `src/api/pdf.rs`
- フロントエンド: `frontend/src/routes/+page.svelte` (1画面のみ)
