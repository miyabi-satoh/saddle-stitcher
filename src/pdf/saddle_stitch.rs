//! 中綴じ製本レイアウトへの変換。
//!
//! 旧 `Tauri-NextTS-SaddleStitcher` の `src-tauri/SaddleStitcher.py` (PyPDF2) を
//! Rust ネイティブ (lopdf) に移植したもの。ページの並べ替えアルゴリズム自体は
//! `new_page_index` として一字一句忠実に移植し、正しさの検証や修正は行わない
//! (旧 README にも「右開きのページ順は未確認」とある通り)。
//!
//! 前提 (旧アプリと同じ):
//! - 全ページが同じサイズ (1ページ目の `MediaBox` を全体で使い回す)
//! - パスワード付き (空パスワードで復号できない) PDF は非対応

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

/// 開いたときにページがどちらへ流れるか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
}

#[derive(Debug, thiserror::Error)]
pub enum SaddleStitchError {
    #[error("PDFとして読み込めませんでした")]
    Load(#[from] lopdf::Error),
    #[error("ページがありません")]
    Empty,
    #[error("ページサイズ(MediaBox)を取得できませんでした")]
    MissingMediaBox,
    #[error("PDFの書き出しに失敗しました")]
    Save(#[from] std::io::Error),
}

/// `SaddleStitcher.py` の `new_page_index()` の移植。
///
/// 4ページ単位の折丁になるよう、末尾を空白ページで埋めた上での中綴じ順序
/// (「詰め込み後のページ列」に対するインデックス列) を返す。
/// 例: 4ページ・左開き → `[3, 0, 1, 2]`
fn new_page_index(pages: usize, direction: Direction) -> Vec<usize> {
    let last_page = pages.div_ceil(4) * 4 - 1;

    let mut page_index = Vec::with_capacity(last_page + 1);
    let mut i = 0usize;
    let mut j = last_page;
    while i <= j {
        page_index.push(j);
        // Python 版は `j = j - 1` の後に無条件で `i` も append する
        // (ループ条件は次の反復の先頭でしか見ない)。
        if j == 0 {
            break;
        }
        j -= 1;
        page_index.push(i);
        i += 1;
    }

    let start = match direction {
        Direction::Right => 0,
        Direction::Left => 2,
    };
    let mut k = start;
    while k < last_page {
        page_index.swap(k, k + 1);
        k += 4;
    }

    page_index
}

/// 元ページ列 (0-indexed, 実ページ数 `pages`) を `new_page_index` の順序で並べ替え、
/// 4の倍数に満たない分を `None` (空白ページ) で埋めた列を返す。
/// 添字は「中綴じ折丁における出力ページ位置」。
fn padded_order(pages: usize, direction: Direction) -> Vec<Option<usize>> {
    let page_index = new_page_index(pages, direction);
    let padded_len = page_index.len();
    // pdf_tmp[k] = 実ページ k (k < pages) または 空白 (k >= pages)
    let padded: Vec<Option<usize>> = (0..padded_len)
        .map(|k| if k < pages { Some(k) } else { None })
        .collect();
    page_index.into_iter().map(|idx| padded[idx]).collect()
}

/// PDF バイト列を読み込み、中綴じ見開きレイアウトの PDF バイト列を返す。
pub fn saddle_stitch(input: &[u8], direction: Direction) -> Result<Vec<u8>, SaddleStitchError> {
    let mut doc = Document::load_mem(input)?;

    let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let num_pages = page_ids.len();
    if num_pages == 0 {
        return Err(SaddleStitchError::Empty);
    }

    // 全ページ共通のページサイズとして1ページ目の MediaBox を採用する。
    let media_box = media_box_of(&doc, page_ids[0])?;
    let page_width = media_box[2] - media_box[0];
    let page_height = media_box[3] - media_box[1];

    // 各実ページを Form XObject 化しておく (内容はそのまま、配置だけを新しいページで行う)。
    let form_ids: Vec<ObjectId> = page_ids
        .iter()
        .map(|&page_id| form_xobject_from_page(&mut doc, page_id, media_box))
        .collect::<Result<_, _>>()?;

    let order = padded_order(num_pages, direction);
    debug_assert_eq!(order.len() % 2, 0, "padded_order は常に偶数長のはず");

    // 見開き2ページずつ、幅2倍の新しいページにまとめる。
    let pages_id = doc.new_object_id();
    let mut spread_ids = Vec::with_capacity(order.len() / 2);
    for pair in order.chunks(2) {
        let left = pair[0].map(|i| form_ids[i]);
        let right = pair[1].map(|i| form_ids[i]);
        let spread_id = new_spread_page(&mut doc, pages_id, page_width, page_height, left, right);
        spread_ids.push(spread_id);
    }

    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Count" => spread_ids.len() as i64,
        "Kids" => spread_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    // 元の Catalog / Outlines / AcroForm 等は差し替え後のページ構成と整合しないため引き継がない。
    // (旧 Python 版も PdfFileWriter で新規に組み直しており、元文書のしおり等は保持していなかった)
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    doc.compress();

    let mut buffer = Vec::new();
    doc.save_to(&mut buffer)?;
    Ok(buffer)
}

/// ページの `MediaBox` を `[llx, lly, urx, ury]` として取得する。
/// ページ自身に無ければ親 `Pages` ノードを辿って継承値を探す (PDF の仕様通り)。
fn media_box_of(doc: &Document, page_id: ObjectId) -> Result<[f32; 4], SaddleStitchError> {
    let mut current = Some(page_id);
    while let Some(id) = current {
        let dict = doc
            .get_dictionary(id)
            .map_err(|_| SaddleStitchError::MissingMediaBox)?;
        if let Ok(array) = dict.get(b"MediaBox").and_then(Object::as_array) {
            let values: Vec<f32> = array
                .iter()
                .filter_map(|obj| {
                    obj.as_float()
                        .or_else(|_| obj.as_i64().map(|v| v as f32))
                        .ok()
                })
                .collect();
            if values.len() == 4 {
                return Ok([values[0], values[1], values[2], values[3]]);
            }
        }
        current = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    Err(SaddleStitchError::MissingMediaBox)
}

/// 1ページ分の内容を Form XObject 化する。継承されたリソースも含めて引き継ぐ。
fn form_xobject_from_page(
    doc: &mut Document,
    page_id: ObjectId,
    media_box: [f32; 4],
) -> Result<ObjectId, SaddleStitchError> {
    let content = doc.get_page_content(page_id);

    let resources = page_resources(doc, page_id);

    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "FormType" => 1,
        "BBox" => media_box.iter().map(|&v| v.into()).collect::<Vec<Object>>(),
        "Resources" => resources,
    };
    dict.remove(b"Length");

    let stream = Stream::new(dict, content);
    Ok(doc.add_object(stream))
}

/// ページの `Resources` を継承分も合わせて解決する。見つからなければ空の辞書を返す。
fn page_resources(doc: &Document, page_id: ObjectId) -> Dictionary {
    match doc.get_page_resources(page_id) {
        Ok((Some(dict), _inherited)) => dict.clone(),
        Ok((None, _)) | Err(_) => Dictionary::new(),
    }
}

/// 見開き1枚分の新規ページを作る。`left`/`right` は Form XObject の ObjectId
/// (`None` なら何も描画しない = 空白)。
fn new_spread_page(
    doc: &mut Document,
    parent: ObjectId,
    page_width: f32,
    page_height: f32,
    left: Option<ObjectId>,
    right: Option<ObjectId>,
) -> ObjectId {
    let mut resources = Dictionary::new();
    let mut xobjects = Dictionary::new();
    let mut operations = Vec::new();

    if let Some(id) = left {
        let name = format!("X{}", id.0);
        xobjects.set(name.clone(), Object::Reference(id));
        push_placement(&mut operations, &name, 0.0, 0.0);
    }
    if let Some(id) = right {
        let name = format!("X{}", id.0);
        xobjects.set(name.clone(), Object::Reference(id));
        push_placement(&mut operations, &name, page_width, 0.0);
    }
    resources.set("XObject", xobjects);

    let content = Content { operations }.encode().unwrap_or_default();
    let content_id = doc.add_object(Stream::new(Dictionary::new(), content));

    let page_dict = dictionary! {
        "Type" => "Page",
        "Parent" => parent,
        "MediaBox" => vec![0.into(), 0.into(), (page_width * 2.0).into(), page_height.into()],
        "Resources" => resources,
        "Contents" => content_id,
    };
    doc.add_object(page_dict)
}

/// `q / cm(平行移動のみ) / Do / Q` で Form XObject を指定位置に配置する。
fn push_placement(operations: &mut Vec<Operation>, name: &str, x: f32, y: f32) {
    operations.push(Operation::new("q", vec![]));
    operations.push(Operation::new(
        "cm",
        vec![1.into(), 0.into(), 0.into(), 1.into(), x.into(), y.into()],
    ));
    operations.push(Operation::new(
        "Do",
        vec![Object::Name(name.as_bytes().to_vec())],
    ));
    operations.push(Operation::new("Q", vec![]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_pages_left_open() {
        assert_eq!(new_page_index(4, Direction::Left), vec![3, 0, 1, 2]);
    }

    #[test]
    fn four_pages_right_open() {
        assert_eq!(new_page_index(4, Direction::Right), vec![0, 3, 2, 1]);
    }

    #[test]
    fn non_multiple_of_four_pads_with_blanks() {
        // 5ページ -> 8ページ分(2の倍数の折丁)に空白ページを足して並べ替える
        let index = new_page_index(5, Direction::Left);
        assert_eq!(index.len(), 8);
        assert_eq!(index, vec![7, 0, 1, 6, 5, 2, 3, 4]);
    }

    /// A4 縦 (595x842pt) 相当のテキストのみの `pages` ページの PDF をその場で作る。
    fn sample_pdf(pages: usize) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::with_capacity(pages);
        for n in 0..pages {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Td", vec![72.into(), 700.into()]),
                    Operation::new("Tj", vec![Object::string_literal(format!("page {n}"))]),
                    Operation::new("ET", vec![]),
                ],
            }
            .encode()
            .unwrap();
            let content_id = doc.add_object(Stream::new(Dictionary::new(), content));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Resources" => dictionary! {
                    "Font" => dictionary! {
                        "F1" => dictionary! {
                            "Type" => "Font",
                            "Subtype" => "Type1",
                            "BaseFont" => "Helvetica",
                        },
                    },
                },
                "Contents" => content_id,
            });
            kids.push(Object::Reference(page_id));
        }
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Count" => pages as i64,
                "Kids" => kids,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let mut buffer = Vec::new();
        doc.save_to(&mut buffer).unwrap();
        buffer
    }

    #[test]
    fn end_to_end_produces_a_loadable_pdf_with_expected_page_count() {
        let input = sample_pdf(4);

        let output = saddle_stitch(&input, Direction::Left).expect("saddle_stitch should succeed");

        let out_doc = Document::load_mem(&output).expect("output should be a valid PDF");
        let out_pages = out_doc.get_pages();
        // 4ページ -> 2見開き (各見開きは幅2倍・高さ同じの1ページ)
        assert_eq!(out_pages.len(), 2);
        for (_, page_id) in out_pages {
            let media_box = media_box_of(&out_doc, page_id).unwrap();
            assert_eq!(media_box, [0.0, 0.0, 595.0 * 2.0, 842.0]);
        }
    }

    #[test]
    fn end_to_end_non_multiple_of_four_pads_with_blank_spread() {
        let input = sample_pdf(5);

        let output = saddle_stitch(&input, Direction::Left).expect("saddle_stitch should succeed");

        let out_doc = Document::load_mem(&output).expect("output should be a valid PDF");
        // 5ページ -> 8ページ分に空白パディング -> 4見開き
        assert_eq!(out_doc.get_pages().len(), 4);
    }

    #[test]
    fn padded_order_marks_blank_pages_as_none() {
        let order = padded_order(5, Direction::Left);
        // 実ページ数5に対し、インデックス5,6,7は空白ページ(=None)になるはず
        assert_eq!(
            order,
            vec![
                None,
                Some(0),
                Some(1),
                None,
                None,
                Some(2),
                Some(3),
                Some(4)
            ]
        );
    }
}
