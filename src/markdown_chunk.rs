//! Markdown-aware message chunking.
//!
//! Discord caps individual messages at 2000 characters, so long generations have
//! to be projected across several messages. A naive split (e.g. by words) produces
//! ugly artifacts: lines broken mid-sentence, code fences left open, bold/spoiler
//! spans that lose their formatting, list items whose continuation loses its
//! indentation, and so on.
//!
//! [`chunk_message`] instead splits on line boundaries (never mid-line unless a
//! single line is itself too long) and tracks the formatting *context* that is
//! active at each split point. When a chunk ends while a context is still open --
//! inside a code block, a `>>>` block quote, a `**bold**` span, etc. -- the chunk
//! is closed cleanly and the context is reopened at the start of the next chunk.
//! When a single line is too long to fit and must be wrapped, continuation lines
//! are indented to stay aligned under their list marker / block quote.

/// Inline markers we track across chunk boundaries, longest-first so that e.g.
/// `**` is matched in preference to `*`.
const MARKERS: [&str; 8] = ["***", "||", "~~", "**", "__", "*", "_", "`"];

/// Split a Markdown `message` into chunks that each render within `chunk_size`
/// characters, preferring line boundaries and preserving formatting context
/// across chunk boundaries.
pub fn chunk_message(message: &str, chunk_size: usize) -> Vec<String> {
    let mut chunker = Chunker::new(chunk_size);
    for line in message.split('\n') {
        chunker.add_logical_line(line);
    }
    chunker.finish()
}

/// Formatting context that must be restored when a chunk continues into the next
/// message.
#[derive(Clone, Default, PartialEq, Eq)]
struct Context {
    /// The opening fence (e.g. ` ```rust `) of a code block we're currently inside.
    code_fence: Option<String>,
    /// Whether we're inside a `>>>` multi-line block quote.
    multiline_quote: bool,
    /// Stack of currently-open inline markers, outermost first.
    inline: Vec<&'static str>,
}

impl Context {
    /// Prefix conceptually prepended to a chunk that begins while this context is
    /// active. Actual rendering is done by [`render_chunk`], which places the
    /// inline markers more carefully; this is kept as the length oracle the size
    /// accounting relies on (it has the same length as what is rendered).
    fn reopen_prefix(&self) -> String {
        let mut s = String::new();
        if self.multiline_quote {
            s.push_str(">>> ");
        }
        if let Some(fence) = &self.code_fence {
            s.push_str(fence);
            s.push('\n');
        }
        for marker in &self.inline {
            s.push_str(marker);
        }
        s
    }

    /// Suffix appended to a chunk that ends while this context is active, so the
    /// chunk is self-contained and doesn't leak an unterminated span.
    fn close_suffix(&self) -> String {
        let mut s = String::new();
        for marker in self.inline.iter().rev() {
            s.push_str(marker);
        }
        if self.code_fence.is_some() {
            s.push_str("\n```");
        }
        s
    }

    /// Update the context after consuming `line`.
    fn observe(&mut self, line: &str) {
        let trimmed = line.trim_start();

        if self.code_fence.is_some() {
            // Inside a code block, only a closing fence is meaningful.
            if trimmed.starts_with("```") {
                self.code_fence = None;
            }
            return;
        }

        if trimmed.starts_with("```") {
            // Entering a code block: its contents are literal, so abandon any open
            // inline spans rather than carrying them through (and into) the block.
            self.inline.clear();
            self.code_fence = Some(trimmed.trim_end().to_string());
            return;
        }

        if line == ">>>" || line.starts_with(">>> ") {
            self.multiline_quote = true;
        }

        // Scan for inline markers, but skip the leading block marker (list bullet,
        // `> ` quote, indentation) so that e.g. a `* ` bullet isn't mistaken for
        // the start of an italic span.
        let (prefix, _) = block_prefixes(line);
        scan_inline(&line[prefix.len()..], &mut self.inline);

        // Inline code (single backtick) doesn't span newlines in Discord: an
        // unclosed backtick renders literally, so it shouldn't be carried over.
        while self.inline.last() == Some(&"`") {
            self.inline.pop();
        }
    }
}

/// Scan a line for inline markers, updating the open-marker `stack`.
///
/// A marker that matches the top of the stack closes it; any other marker opens a
/// new span. Emphasis markers follow Discord's "no space against the marker" rule
/// (and underscores additionally require word boundaries) so that arithmetic like
/// `2 * 3` and identifiers like `snake_case` aren't treated as formatting. While
/// inside an inline-code span everything but a closing backtick is literal.
fn scan_inline(line: &str, stack: &mut Vec<&'static str>) {
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];

        if stack.last() == Some(&"`") {
            if rest.starts_with('`') {
                stack.pop();
                i += 1;
            } else {
                i += next_char_len(rest);
            }
            continue;
        }

        let Some(marker) = MARKERS.iter().copied().find(|m| rest.starts_with(m)) else {
            i += next_char_len(rest);
            continue;
        };

        let closes = stack.last() == Some(&marker);
        let before = line[..i].chars().next_back();
        let after = line[i + marker.len()..].chars().next();

        if marker_applies(marker, closes, before, after) {
            if closes {
                stack.pop();
            } else {
                stack.push(marker);
            }
        }
        i += marker.len();
    }
}

/// Whether a candidate `marker` should be treated as formatting given the
/// characters immediately `before` and `after` it and whether it would `close` an
/// open span.
fn marker_applies(marker: &str, closes: bool, before: Option<char>, after: Option<char>) -> bool {
    // Inline code toggles freely; spoilers and emphasis must hug their content.
    if marker == "`" {
        return true;
    }
    let underscore = matches!(marker, "_" | "__");

    if closes {
        // A closing marker must not have whitespace just inside the span. At the
        // very start of a line `before` is `None`: the span's content is on the
        // previous line, so that's still a valid close, not a whitespace flank.
        if before.is_some_and(char::is_whitespace) {
            return false;
        }
        if underscore && after.is_some_and(|c| c.is_alphanumeric()) {
            return false;
        }
    } else {
        // An opening marker must not have whitespace just inside the span.
        if after.is_none_or(char::is_whitespace) {
            return false;
        }
        if underscore && before.is_some_and(|c| c.is_alphanumeric()) {
            return false;
        }
    }
    true
}

fn next_char_len(s: &str) -> usize {
    s.chars().next().map_or(1, char::len_utf8)
}

/// Determine the leading block markers of `line`, returning the prefix to keep on
/// its first (wrapped) piece and the prefix to indent continuation pieces with.
///
/// For a bullet `- text` this is `("- ", "  ")`; for `> quote` it's `("> ", "> ")`;
/// for a nested `  - text` it's `("  - ", "    ")`; for a plain (possibly indented)
/// line the indentation is preserved on continuations.
fn block_prefixes(line: &str) -> (String, String) {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);

    if rest.starts_with("> ") {
        let p = format!("{indent}> ");
        return (p.clone(), p);
    }

    for marker in ["- ", "* ", "+ "] {
        if rest.starts_with(marker) {
            let first = format!("{indent}{marker}");
            let cont = " ".repeat(indent.len() + marker.len());
            return (first, cont);
        }
    }

    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let after = &rest[digits..];
        if after.starts_with(". ") || after.starts_with(") ") {
            let marker_len = digits + 2;
            let first = format!("{indent}{}", &rest[..marker_len]);
            let cont = " ".repeat(indent.len() + marker_len);
            return (first, cont);
        }
    }

    (indent.to_string(), indent.to_string())
}

struct Chunker {
    chunk_size: usize,
    chunks: Vec<String>,
    /// Context entering the chunk currently being built.
    start_ctx: Context,
    /// Running context after the lines accumulated so far.
    ctx: Context,
    /// Raw lines accumulated for the current chunk.
    body: Vec<String>,
}

impl Chunker {
    fn new(chunk_size: usize) -> Self {
        Self {
            chunk_size,
            chunks: Vec::new(),
            start_ctx: Context::default(),
            ctx: Context::default(),
            body: Vec::new(),
        }
    }

    /// Rendered length of a hypothetical fresh chunk containing exactly `content`,
    /// given the context that would be carried into it.
    fn fresh_chunk_len(&self, content: &str) -> usize {
        let mut projected = self.ctx.clone();
        projected.observe(content);
        self.ctx.reopen_prefix().len() + content.len() + projected.close_suffix().len()
    }

    /// Add a source line, word-wrapping it first if it can't fit on its own.
    fn add_logical_line(&mut self, line: &str) {
        if self.fresh_chunk_len(line) <= self.chunk_size {
            self.add_line(line);
        } else {
            self.wrap_line(line);
        }
    }

    /// Add a line that is known to fit on its own, splitting to a new chunk if it
    /// can't share the current one.
    fn add_line(&mut self, line: &str) {
        // Drop blank lines at the very start of a chunk so continuations don't
        // begin with stray empty lines -- unless we're inside a code block, where
        // blank lines are significant.
        if self.body.is_empty() && line.trim().is_empty() && self.ctx.code_fence.is_none() {
            return;
        }

        let mut projected = self.ctx.clone();
        projected.observe(line);

        let projected_len = self.start_ctx.reopen_prefix().len()
            + body_len_with(&self.body, line)
            + projected.close_suffix().len();

        if !self.body.is_empty() && projected_len > self.chunk_size {
            self.finalize();
            let mut carried = self.ctx.clone();
            carried.observe(line);
            self.body.push(line.to_string());
            self.ctx = carried;
        } else {
            self.body.push(line.to_string());
            self.ctx = projected;
        }
    }

    /// Word-wrap an over-long line across its own dedicated chunks, keeping list /
    /// block-quote indentation on the continuation lines.
    fn wrap_line(&mut self, line: &str) {
        if !self.body.is_empty() {
            self.finalize();
        }

        let (first_prefix, cont_prefix) = block_prefixes(line);
        let content = line[first_prefix.len()..].to_string();

        // A line that is nothing but its block marker (e.g. a bare `- ` or `1. `)
        // has no content to wrap. Emit it whole rather than letting the empty-word
        // loop below drop the marker -- which can carry alphanumerics (e.g. the `1`)
        // and would be silent content loss. `enforce_size` clamps if it overflows.
        if content.is_empty() {
            self.add_line(line);
            return;
        }

        let mut prefix = first_prefix;
        let mut piece = String::new();

        for word in content.split(' ') {
            let grown = if piece.is_empty() {
                word.to_string()
            } else {
                format!("{piece} {word}")
            };
            if self.fresh_chunk_len(&format!("{prefix}{grown}")) <= self.chunk_size {
                piece = grown;
                continue;
            }

            // `word` doesn't fit on the current piece; flush what we have.
            if !piece.is_empty() {
                let display = format!("{prefix}{piece}");
                self.emit_chunk(&display);
                prefix = cont_prefix.clone();
                piece.clear();
            }

            if self.fresh_chunk_len(&format!("{prefix}{word}")) <= self.chunk_size {
                piece = word.to_string();
            } else {
                // A single word longer than a whole chunk: hard-split it.
                piece = self.emit_word_hard(&prefix, &cont_prefix, word);
                prefix = cont_prefix.clone();
            }
        }

        if !piece.is_empty() {
            // Leave the trailing piece in the body so a following source line can
            // share its chunk.
            self.add_line(&format!("{prefix}{piece}"));
        }
    }

    /// Hard-split a word that is longer than a whole chunk, emitting full chunks
    /// and returning the (unemitted) remainder.
    fn emit_word_hard(&mut self, first_prefix: &str, cont_prefix: &str, word: &str) -> String {
        let mut prefix = first_prefix.to_string();
        let mut cur = String::new();
        for c in word.chars() {
            let grown = format!("{cur}{c}");
            if !cur.is_empty()
                && self.fresh_chunk_len(&format!("{prefix}{grown}")) > self.chunk_size
            {
                self.emit_chunk(&format!("{prefix}{cur}"));
                prefix = cont_prefix.to_string();
                cur = c.to_string();
            } else {
                cur = grown;
            }
        }
        cur
    }

    /// Emit `content` as a standalone chunk.
    fn emit_chunk(&mut self, content: &str) {
        debug_assert!(self.body.is_empty());
        self.add_line(content);
        self.finalize();
    }

    /// Emit the current chunk and carry its end context into the next one.
    fn finalize(&mut self) {
        // Trim trailing blank lines so a chunk doesn't end with stray empty lines
        // -- unless we're inside a code block, where blank lines are significant.
        if self.ctx.code_fence.is_none() {
            while self.body.last().is_some_and(|l| l.trim().is_empty()) {
                self.body.pop();
            }
        }

        let rendered = render_chunk(&self.start_ctx, &self.body, &self.ctx);
        if !rendered.trim().is_empty() {
            self.chunks.push(rendered);
        }

        self.start_ctx = self.ctx.clone();
        self.body.clear();
    }

    fn finish(mut self) -> Vec<String> {
        self.finalize();

        // Safety net: in pathological cases (e.g. a tiny chunk size against deeply
        // nested formatting) the unavoidable reopen/close overhead can exceed the
        // chunk size on its own. Discord hard-rejects oversized messages, so as a
        // last resort hard-split any chunk that still doesn't fit. This never fires
        // at realistic sizes.
        let chunks = enforce_size(self.chunks, self.chunk_size);

        if chunks.is_empty() {
            return vec![String::new()];
        }
        chunks
    }
}

/// Hard-split any chunk that exceeds `size` on character boundaries. Every returned
/// chunk is within the limit, except in the degenerate case where a single `char`
/// is itself longer than `size` (only possible for `size < 4`, never at Discord's
/// limits) -- a `char` is never split.
fn enforce_size(chunks: Vec<String>, size: usize) -> Vec<String> {
    if chunks.iter().all(|c| c.len() <= size) {
        return chunks;
    }

    let mut out = Vec::new();
    for chunk in chunks {
        if chunk.len() <= size {
            out.push(chunk);
            continue;
        }
        let mut cur = String::new();
        for c in chunk.chars() {
            if !cur.is_empty() && cur.len() + c.len_utf8() > size {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out
}

/// Render a chunk: reopen the carried context, emit the body, then close whatever
/// is still open.
///
/// Inline markers are reopened *after* the first line's leading block marker, so a
/// carried `**` doesn't clobber a `- ` bullet or `>>> ` quote into `**- ` / `**>>> `.
/// They are dropped entirely when the chunk starts inside a code block or its first
/// line opens one, because a code block's contents are literal and a marker glued
/// to a fence would break it.
///
/// The total length matches [`Context::reopen_prefix`] + body +
/// [`Context::close_suffix`] (which the size accounting relies on) in every case
/// except the code-block drop, where the rendered chunk is only ever shorter.
fn render_chunk(start: &Context, body: &[String], end: &Context) -> String {
    let mut lines: Vec<String> = body.to_vec();

    if start.code_fence.is_none()
        && !start.inline.is_empty()
        && let Some(first) = lines.first_mut()
        && let Some(idx) = inline_insertion_index(first)
    {
        first.insert_str(idx, &start.inline.concat());
    }

    let mut rendered = String::new();
    if start.multiline_quote {
        rendered.push_str(">>> ");
    }
    if let Some(fence) = &start.code_fence {
        rendered.push_str(fence);
        rendered.push('\n');
    }
    rendered.push_str(&lines.join("\n"));

    for marker in end.inline.iter().rev() {
        rendered.push_str(marker);
    }
    if end.code_fence.is_some() {
        rendered.push_str("\n```");
    }

    rendered
}

/// Index in `line` at which carried inline markers may be reinserted, or `None` if
/// the line must not be decorated at all (it opens a code block).
fn inline_insertion_index(line: &str) -> Option<usize> {
    if line.trim_start().starts_with("```") {
        return None;
    }
    if line.starts_with(">>> ") {
        return Some(4);
    }
    if line == ">>>" {
        // Insert after the bare marker (not before it, which would corrupt it into
        // `**>>>`), keeping the chunk's markers balanced.
        return Some(3);
    }
    Some(block_prefixes(line).0.len())
}

/// Length of `body` joined with newlines, with `extra` appended as a new line.
fn body_len_with(body: &[String], extra: &str) -> usize {
    let mut len: usize = body.iter().map(String::len).sum();
    if !body.is_empty() {
        len += body.len() - 1; // newlines between existing lines
        len += 1; // newline before `extra`
    }
    len + extra.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_within_size(chunks: &[String], chunk_size: usize) {
        for chunk in chunks {
            assert!(
                chunk.len() <= chunk_size,
                "chunk exceeds size {chunk_size}: {} chars\n{chunk:?}",
                chunk.len()
            );
        }
    }

    fn assert_chunks_nonempty(chunks: &[String]) {
        for chunk in chunks {
            assert!(!chunk.trim().is_empty(), "empty chunk emitted: {chunk:?}");
        }
    }

    /// Letters and digits only, so we can compare content ignoring any markers,
    /// indentation, or newlines the chunker added or moved.
    fn alnum(s: &str) -> String {
        s.chars().filter(|c| c.is_alphanumeric()).collect()
    }

    // --- basics ----------------------------------------------------------------

    #[test]
    fn short_message_is_single_chunk() {
        assert_eq!(chunk_message("hello world", 1500), vec!["hello world"]);
    }

    #[test]
    fn empty_message_yields_one_empty_chunk() {
        assert_eq!(chunk_message("", 1500), vec![String::new()]);
    }

    #[test]
    fn whitespace_only_message_is_not_emitted_as_garbage() {
        let chunks = chunk_message("   \n  \n", 1500);
        assert_eq!(chunks, vec![String::new()]);
    }

    #[test]
    fn trailing_newline_does_not_create_empty_chunk() {
        let chunks = chunk_message("hello\n", 1500);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn splits_on_line_boundaries_not_mid_line() {
        let line = "a".repeat(40);
        let message = vec![line.clone(); 10].join("\n");
        let chunks = chunk_message(&message, 100);

        assert!(chunks.len() > 1);
        assert_within_size(&chunks, 100);
        for chunk in &chunks {
            for chunk_line in chunk.split('\n') {
                assert!(chunk_line.is_empty() || chunk_line == line);
            }
        }
    }

    // --- code blocks -----------------------------------------------------------

    #[test]
    fn closes_and_reopens_code_block_with_language() {
        let body = (0..20)
            .map(|i| format!("let x{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n");
        let message = format!("```rust\n{body}\n```");
        let chunks = chunk_message(&message, 120);

        assert!(chunks.len() > 1);
        assert_within_size(&chunks, 120);
        for chunk in &chunks {
            assert_eq!(
                chunk.matches("```").count() % 2,
                0,
                "unbalanced fence: {chunk}"
            );
            assert!(chunk.starts_with("```rust"), "lost language: {chunk}");
        }
    }

    #[test]
    fn code_block_without_language_reopens_bare() {
        let body = vec!["some code line".to_string(); 30].join("\n");
        let message = format!("```\n{body}\n```");
        let chunks = chunk_message(&message, 120);

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.starts_with("```"), "lost fence: {chunk}");
            assert_eq!(
                chunk.matches("```").count() % 2,
                0,
                "unbalanced fence: {chunk}"
            );
        }
    }

    #[test]
    fn preserves_blank_lines_inside_code_block() {
        let message = "```\nfirst\n\n\nsecond\n```";
        let chunks = chunk_message(message, 1500);
        assert_eq!(chunks, vec![message.to_string()]);
    }

    #[test]
    fn handles_two_separate_code_blocks() {
        let message = "```rust\nlet a = 1;\n```\nprose\n```py\nb = 2\n```";
        let chunks = chunk_message(message, 1500);
        assert_eq!(chunks, vec![message.to_string()]);
    }

    // --- inline spans ----------------------------------------------------------

    #[test]
    fn carries_bold_across_a_split() {
        let line = "x".repeat(60);
        let message = format!("**{line}\n{line}\n{line}**");
        let chunks = chunk_message(&message, 100);

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert_eq!(
                chunk.matches("**").count() % 2,
                0,
                "unbalanced bold: {chunk}"
            );
        }
        assert!(chunks[0].ends_with("**"));
        assert!(chunks[1].starts_with("**"));
    }

    #[test]
    fn carries_italic_across_a_split() {
        let line = "y".repeat(60);
        let message = format!("*{line}\n{line}\n{line}*");
        let chunks = chunk_message(&message, 100);

        assert!(chunks.len() > 1);
        assert!(chunks[0].ends_with('*'));
        assert!(chunks[1].starts_with('*'));
    }

    #[test]
    fn carries_strikethrough_and_spoiler_and_underline() {
        for marker in ["~~", "||", "__"] {
            let line = "z".repeat(60);
            let message = format!("{marker}{line}\n{line}\n{line}{marker}");
            let chunks = chunk_message(&message, 100);

            assert!(chunks.len() > 1, "marker {marker} did not split");
            for chunk in &chunks {
                assert_eq!(
                    chunk.matches(marker).count() % 2,
                    0,
                    "unbalanced {marker}: {chunk}"
                );
            }
        }
    }

    #[test]
    fn inline_code_suppresses_markers() {
        let message = "here is `some **code**` and then plain text";
        assert_eq!(chunk_message(message, 1500), vec![message.to_string()]);
    }

    #[test]
    fn unclosed_inline_code_is_not_carried() {
        // A lone backtick renders literally in Discord and must not be carried.
        let message = "a line with one ` backtick\nand a following line";
        let chunks = chunk_message(message, 1500);
        assert_eq!(chunks, vec![message.to_string()]);
    }

    // --- flanking / false positives -------------------------------------------

    #[test]
    fn arithmetic_asterisks_are_not_italic() {
        let message = "compute 2 * 3 * 4 and you get 24";
        assert_eq!(chunk_message(message, 1500), vec![message.to_string()]);
    }

    #[test]
    fn snake_case_is_not_emphasis() {
        let message = "call some_long_function_name(with_an_arg) now";
        assert_eq!(chunk_message(message, 1500), vec![message.to_string()]);
    }

    #[test]
    fn dunder_identifier_is_balanced() {
        // Even if treated as underline, it's balanced within the line, so nothing
        // leaks across a boundary.
        let message = "the __init__ method and __repr__ method";
        assert_eq!(chunk_message(message, 1500), vec![message.to_string()]);
    }

    #[test]
    fn star_bullet_is_not_treated_as_italic() {
        let items = (0..40)
            .map(|i| format!("* item number {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = chunk_message(&items, 200);

        assert!(chunks.len() > 1);
        // No spurious italic markers should have been injected.
        for chunk in &chunks {
            assert!(!chunk.contains("**"), "spurious markers: {chunk}");
        }
    }

    // --- list / block-quote indentation on wrap -------------------------------

    #[test]
    fn wrapped_bullet_keeps_continuation_indent() {
        let long = (0..60)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let message = format!("- {long}");
        let chunks = chunk_message(&message, 120);

        assert!(chunks.len() > 1);
        assert_within_size(&chunks, 120);
        assert!(
            chunks[0].starts_with("- "),
            "first piece lost bullet: {}",
            chunks[0]
        );
        for chunk in &chunks[1..] {
            assert!(
                chunk.starts_with("  ") && !chunk.starts_with("- "),
                "continuation lost indent: {chunk:?}"
            );
        }
        assert_eq!(alnum(&chunks.concat()), alnum(&message));
    }

    #[test]
    fn wrapped_ordered_item_indents_to_marker_width() {
        let long = (0..60)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let message = format!("12. {long}");
        let chunks = chunk_message(&message, 120);

        assert!(chunks.len() > 1);
        assert!(chunks[0].starts_with("12. "));
        for chunk in &chunks[1..] {
            // "12. " is four columns wide.
            assert!(chunk.starts_with("    "), "wrong indent: {chunk:?}");
        }
    }

    #[test]
    fn wrapped_nested_bullet_keeps_deeper_indent() {
        let long = (0..60)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let message = format!("  - {long}");
        let chunks = chunk_message(&message, 120);

        assert!(chunks.len() > 1);
        assert!(chunks[0].starts_with("  - "));
        for chunk in &chunks[1..] {
            assert!(chunk.starts_with("    "), "wrong nested indent: {chunk:?}");
        }
    }

    #[test]
    fn wrapped_block_quote_keeps_quote_prefix() {
        let long = (0..60)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let message = format!("> {long}");
        let chunks = chunk_message(&message, 120);

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.starts_with("> "), "lost quote prefix: {chunk:?}");
        }
    }

    #[test]
    fn nested_list_keeps_per_line_indentation() {
        let message = "- top\n  - mid\n    - deep\n- top2";
        let chunks = chunk_message(message, 1500);
        assert_eq!(chunks, vec![message.to_string()]);
    }

    #[test]
    fn open_span_before_code_block_does_not_leak_into_code() {
        // An unclosed `**` before a fenced block must not be carried into the code
        // (its contents are literal) nor glued onto the fence.
        let intro = format!("**{}", "word ".repeat(40));
        let code = (0..20)
            .map(|i| format!("let value_{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n");
        let message = format!("{intro}\n```rust\n{code}\n```");
        let chunks = chunk_message(&message, 150);

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(!chunk.contains("**```"), "marker glued to fence: {chunk:?}");
            assert!(
                !chunk.contains("```rust**"),
                "marker glued to fence: {chunk:?}"
            );
            for line in chunk.lines() {
                if line.contains("let value_") {
                    assert!(!line.contains("**"), "marker leaked into code: {line:?}");
                }
            }
        }
    }

    #[test]
    fn open_span_carried_into_list_keeps_marker() {
        // A `**` carried across into a chunk that starts with a bullet must land
        // after the `- `, not before it.
        let message = format!(
            "**{}\n- bullet item alpha\n- bullet item beta",
            "word ".repeat(40)
        );
        let chunks = chunk_message(&message, 120);

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(
                !chunk.contains("**-"),
                "bold clobbered bullet marker: {chunk:?}"
            );
        }
        // Somewhere the bold should have been reopened just inside a bullet.
        assert!(
            chunks.iter().any(|c| c.contains("- **")),
            "bold not reopened inside the bullet: {chunks:?}"
        );
    }

    #[test]
    fn closing_marker_at_line_start_closes_the_span() {
        let message = "**bold text spanning\n** then plain text";
        assert_eq!(chunk_message(message, 1500), vec![message.to_string()]);
    }

    #[test]
    fn carries_multiline_blockquote() {
        let body = vec!["quoted text here".to_string(); 40].join("\n");
        let message = format!(">>> {body}");
        let chunks = chunk_message(&message, 200);

        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.starts_with(">>> "), "lost block quote: {chunk:?}");
        }
    }

    // --- wrapping long lines ---------------------------------------------------

    #[test]
    fn wraps_a_single_overlong_line() {
        let message = (0..200)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let chunks = chunk_message(&message, 100);

        assert!(chunks.len() > 1);
        assert_within_size(&chunks, 100);
        assert_eq!(alnum(&chunks.concat()), alnum(&message));
    }

    #[test]
    fn hard_splits_a_word_longer_than_budget() {
        let message = "x".repeat(500);
        let chunks = chunk_message(&message, 100);

        assert!(chunks.len() > 1);
        assert_within_size(&chunks, 100);
        assert_eq!(chunks.concat(), message);
    }

    #[test]
    fn header_lines_are_left_intact() {
        let message = "# Title\n## Subtitle\nbody text";
        assert_eq!(chunk_message(message, 1500), vec![message.to_string()]);
    }

    // --- robustness ------------------------------------------------------------

    #[test]
    fn multibyte_content_does_not_panic_and_stays_within_size() {
        let message = "héllo🎉wörld ".repeat(80);
        let chunks = chunk_message(&message, 100);
        assert!(chunks.len() > 1);
        assert_within_size(&chunks, 100);
    }

    #[test]
    fn tiny_chunk_size_does_not_panic() {
        let message = "**bold** and `code` and a > quote\n- a bullet point here";
        let chunks = chunk_message(message, 5);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn marker_only_line_is_not_dropped_at_tiny_size() {
        // Regression: a line that is just a list marker must not be silently lost
        // even when reopen overhead makes it not fit.
        let chunks = chunk_message("_-\n1. ", 4);
        assert_eq!(alnum(&chunks.concat()), "1");
    }

    #[test]
    fn carried_span_into_bare_triple_quote_stays_balanced() {
        let message = format!("**{}\n>>>", "p".repeat(74));
        let chunks = chunk_message(&message, 80);
        for chunk in &chunks {
            assert_eq!(
                chunk.matches("**").count() % 2,
                0,
                "unbalanced bold: {chunk:?}"
            );
            assert!(
                !chunk.contains("**>>>"),
                "bold clobbered quote marker: {chunk:?}"
            );
        }
    }

    // --- property / fuzz -------------------------------------------------------

    fn xorshift(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    /// Build pseudo-random markdown-ish text. No code fences (their language tag is
    /// intentionally duplicated on reopen, which would break the alnum invariant).
    fn random_message(seed: u64) -> String {
        let tokens = [
            "alpha", "beta", "gamma", "delta", "epsilon", " ", " ", " ", "\n", "\n", "**", "~~",
            "||", "*", "_", "- ", "> ", "  ", ">>> ",
        ];
        let mut state = seed | 1;
        let len = (xorshift(&mut state) % 400) as usize;
        let mut out = String::new();
        for _ in 0..len {
            let tok = tokens[(xorshift(&mut state) as usize) % tokens.len()];
            out.push_str(tok);
        }
        out
    }

    #[test]
    fn fuzz_invariants_hold() {
        for seed in 1..400u64 {
            let message = random_message(seed);
            for &size in &[8usize, 30, 60, 200] {
                let chunks = chunk_message(&message, size);

                assert_within_size(&chunks, size);
                // The only permitted empty chunk is the sole placeholder for input
                // that renders to nothing.
                if chunks != [String::new()] {
                    assert_chunks_nonempty(&chunks);
                }
                assert_eq!(
                    alnum(&chunks.concat()),
                    alnum(&message),
                    "content lost for seed {seed} size {size}\ninput: {message:?}\nout: {chunks:?}"
                );
            }
        }
    }

    #[test]
    fn fuzz_code_blocks_stay_balanced() {
        for seed in 1..200u64 {
            let mut state = seed | 1;
            let lines = (xorshift(&mut state) % 30) as usize;
            let body = (0..lines)
                .map(|i| format!("code line {i} value {}", xorshift(&mut state) % 100))
                .collect::<Vec<_>>()
                .join("\n");
            let message = format!("```rust\n{body}\n```");

            for &size in &[40usize, 90, 160] {
                let chunks = chunk_message(&message, size);
                assert_within_size(&chunks, size);
                for chunk in &chunks {
                    assert_eq!(
                        chunk.matches("```").count() % 2,
                        0,
                        "unbalanced fence for seed {seed} size {size}: {chunk:?}"
                    );
                }
            }
        }
    }
}
