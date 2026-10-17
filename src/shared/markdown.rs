//! Mermaid diagrams embedded in Markdown as fenced code blocks.

use std::ops::Range;

/// A fenced code block with the `mermaid` info string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidBlock {
    /// 1-based line of the opening fence.
    pub line: usize,
    /// Byte offset where the opening fence line starts.
    start: usize,
    /// Opening fence line, without its line ending.
    open_line: String,
    /// Byte range of the lines between the fences.
    content: Range<usize>,
    /// Byte range of the closing fence line, without its line ending.
    close: Range<usize>,
    /// Indentation of the opening fence, stripped from and restored to every
    /// content line so blocks nested in lists round-trip.
    indent: String,
    marker: char,
    /// Number of marker characters in the fence.
    len: usize,
    /// Line ending of the opening fence, used for the lines written back.
    eol: &'static str,
}

impl MermaidBlock {
    /// The diagram source with the fence indentation removed.
    pub fn source(&self, markdown: &str) -> String {
        markdown[self.content.clone()]
            .split_inclusive('\n')
            .map(|line| line.strip_prefix(self.indent.as_str()).unwrap_or(line))
            .collect()
    }
}

/// A Markdown document with one of its Mermaid blocks open for editing.
#[derive(Debug, Clone)]
pub struct Embedding {
    markdown: String,
    block: MermaidBlock,
}

impl Embedding {
    pub fn new(markdown: String, block: MermaidBlock) -> Self {
        Self { markdown, block }
    }

    pub fn source(&self) -> String {
        self.block.source(&self.markdown)
    }

    /// Replaces the block contents with `source` and returns the whole
    /// document. The fences grow when a line of `source` would otherwise
    /// close the block; the rest of the Markdown is kept byte for byte.
    pub fn replace(&mut self, source: &str) -> &str {
        let block = &mut self.block;
        let mut content = String::new();
        for line in source.lines() {
            if !line.trim().is_empty() {
                content.push_str(&block.indent);
            }
            content.push_str(line);
            content.push_str(block.eol);
        }

        let mut close = self.markdown[block.close.clone()].to_string();
        let longest_fence_line = source
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && l.chars().all(|c| c == block.marker))
            .map(str::len)
            .max()
            .unwrap_or(0);
        if longest_fence_line >= block.len {
            block.len = longest_fence_line + 1;
            let fence = block.marker.to_string().repeat(block.len);
            let info = block
                .open_line
                .trim_start()
                .trim_start_matches(block.marker);
            block.open_line = format!("{}{fence}{info}", block.indent);
            let close_indent = &close[..close.len() - close.trim_start().len()];
            close = format!("{close_indent}{fence}");
        }

        let region = format!("{}{}{content}{close}", block.open_line, block.eol);
        self.markdown
            .replace_range(block.start..block.close.end, &region);
        let content_start = block.start + block.open_line.len() + block.eol.len();
        block.content = content_start..content_start + content.len();
        block.close = block.content.end..block.content.end + close.len();
        &self.markdown
    }
}

/// Finds the closed ```` ```mermaid ```` and `~~~mermaid` blocks, in document
/// order. Unclosed fences are ignored.
pub fn mermaid_blocks(markdown: &str) -> Vec<MermaidBlock> {
    let mut blocks = Vec::new();
    // Opening fence of the code block being read, if any.
    let mut open: Option<(Fence, Option<MermaidBlock>)> = None;
    let mut offset = 0;
    for (index, line) in markdown.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += line.len();
        match &mut open {
            None => {
                if let Some(fence) = Fence::parse(line) {
                    let block = (fence.info == "mermaid").then(|| MermaidBlock {
                        line: index + 1,
                        start,
                        open_line: without_eol(line).to_string(),
                        content: offset..offset,
                        close: offset..offset,
                        indent: fence.indent.clone(),
                        marker: fence.marker,
                        len: fence.len,
                        eol: if line.ends_with("\r\n") { "\r\n" } else { "\n" },
                    });
                    open = Some((fence, block));
                }
            }
            Some((fence, block)) => {
                if fence.is_closed_by(line) {
                    if let Some(mut block) = block.take() {
                        block.content.end = start;
                        block.close = start..start + without_eol(line).len();
                        blocks.push(block);
                    }
                    open = None;
                }
            }
        }
    }
    blocks
}

fn without_eol(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

struct Fence {
    indent: String,
    marker: char,
    len: usize,
    info: String,
}

impl Fence {
    fn parse(line: &str) -> Option<Self> {
        let line = without_eol(line);
        let rest = line.trim_start_matches([' ', '\t']);
        let indent = &line[..line.len() - rest.len()];
        let marker = rest.chars().next().filter(|c| matches!(c, '`' | '~'))?;
        let len = rest.chars().take_while(|&c| c == marker).count();
        let info = &rest[len..];
        // A backtick fence's info string may not contain backticks, which
        // would make it inline code.
        if len < 3 || (marker == '`' && info.contains('`')) {
            return None;
        }
        Some(Self {
            indent: indent.to_owned(),
            marker,
            len,
            info: info.split_whitespace().next().unwrap_or("").to_owned(),
        })
    }

    fn is_closed_by(&self, line: &str) -> bool {
        let rest = line.trim();
        let len = rest.chars().take_while(|&c| c == self.marker).count();
        len >= self.len && len == rest.chars().count()
    }
}

/// Diagram sources in pasted text: the Mermaid blocks of a Markdown
/// snippet, or else the whole text.
pub fn pasted_diagrams(text: &str) -> Vec<String> {
    let blocks = mermaid_blocks(text);
    if blocks.is_empty() {
        vec![text.to_owned()]
    } else {
        blocks.iter().map(|block| block.source(text)).collect()
    }
}

/// Whether a file name is that of a Markdown document.
pub fn is_markdown_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".md") || name.ends_with(".markdown")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Title

```mermaid
flowchart TD
    A --> B
```

```rust
fn main() {}
```

- item

  ~~~~ mermaid
  sequenceDiagram

      A->>B: hi
  ~~~~
";

    #[test]
    fn finds_mermaid_blocks_only() {
        let blocks = mermaid_blocks(DOC);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].line, 3);
        assert_eq!(blocks[0].source(DOC), "flowchart TD\n    A --> B\n");
        assert_eq!(blocks[1].line, 14);
        assert_eq!(blocks[1].source(DOC), "sequenceDiagram\n\n    A->>B: hi\n");
    }

    #[test]
    fn ignores_mermaid_fences_inside_other_blocks_and_unclosed_ones() {
        let doc = "````md\n```mermaid\ngraph LR\n```\n````\n\n```mermaid\ngraph TD\n";
        assert!(mermaid_blocks(doc).is_empty());
    }

    #[test]
    fn closing_fence_needs_same_marker_and_length() {
        let doc = "````mermaid\ngraph LR\n```\n~~~~\n````\n";
        let blocks = mermaid_blocks(doc);
        assert_eq!(blocks[0].source(doc), "graph LR\n```\n~~~~\n");
    }

    #[test]
    fn replace_keeps_the_rest_of_the_document() {
        let block = mermaid_blocks(DOC)[1].clone();
        let mut embedding = Embedding::new(DOC.to_owned(), block);
        let out = embedding
            .replace("sequenceDiagram\n    A->>C: bye\n")
            .to_owned();
        assert_eq!(
            out,
            DOC.replace(
                "  sequenceDiagram\n\n      A->>B: hi\n",
                "  sequenceDiagram\n      A->>C: bye\n"
            )
        );
        assert_eq!(embedding.source(), "sequenceDiagram\n    A->>C: bye\n");

        // The block range follows the new contents.
        let again = embedding.replace("sequenceDiagram").to_owned();
        assert!(again.ends_with("  ~~~~ mermaid\n  sequenceDiagram\n  ~~~~\n"));
        assert_eq!(
            mermaid_blocks(&again)[1].source(&again),
            "sequenceDiagram\n"
        );
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let doc = "text\r\n```mermaid\r\ngraph LR\r\n```\r\nend\r\n";
        let block = mermaid_blocks(doc)[0].clone();
        assert_eq!(block.source(doc), "graph LR\r\n");
        let mut embedding = Embedding::new(doc.to_owned(), block);
        assert_eq!(
            embedding.replace("graph TD\n  A --> B\n"),
            "text\r\n```mermaid\r\ngraph TD\r\n  A --> B\r\n```\r\nend\r\n"
        );
    }

    #[test]
    fn grows_fences_that_the_content_would_close() {
        let doc = "```mermaid title\ngraph LR\n  ```\nafter\n";
        let block = mermaid_blocks(doc)[0].clone();
        let mut embedding = Embedding::new(doc.to_owned(), block);
        let out = embedding.replace("graph LR\n%% ```\n```\n").to_owned();
        assert_eq!(
            out,
            "````mermaid title\ngraph LR\n%% ```\n```\n  ````\nafter\n"
        );
        let blocks = mermaid_blocks(&out);
        assert_eq!(blocks[0].source(&out), "graph LR\n%% ```\n```\n");
        // Later edits keep the longer fence.
        assert_eq!(
            embedding.replace("graph TD"),
            "````mermaid title\ngraph TD\n  ````\nafter\n"
        );
    }

    #[test]
    fn pasted_text_is_split_into_mermaid_blocks() {
        assert_eq!(
            pasted_diagrams(DOC),
            [
                "flowchart TD\n    A --> B\n",
                "sequenceDiagram\n\n    A->>B: hi\n"
            ]
        );
        assert_eq!(pasted_diagrams("graph LR\nA-->B"), ["graph LR\nA-->B"]);
    }

    #[test]
    fn recognises_markdown_names() {
        assert!(is_markdown_name("README.md"));
        assert!(is_markdown_name("notes.Markdown"));
        assert!(!is_markdown_name("diagram.mmd"));
    }
}
