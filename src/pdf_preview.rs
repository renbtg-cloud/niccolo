//! Bounded text preview. The original remains with the user.
use serde::Serialize;
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Semaphore,
};

const MAX_PAGES: usize = 20;
pub(crate) const MAX_START_PAGE: usize = 999_999;
const MAX_OUTPUT: u64 = 512 * 1024;
static PREVIEWS: Semaphore = Semaphore::const_new(2);

#[derive(Debug)]
pub(crate) enum PreviewError {
    Invalid,
    Unavailable,
    Busy,
    Timeout,
    TooLarge,
}

#[derive(Serialize)]
pub(crate) struct Page {
    pub number: usize,
    pub text: String,
}

#[derive(Serialize)]
pub(crate) struct Preview {
    pub pages: Vec<Page>,
    pub truncated: bool,
    pub next_page: Option<usize>,
}

pub(crate) async fn extract(bytes: &[u8], first: usize) -> Result<Preview, PreviewError> {
    if !bytes.starts_with(b"%PDF-") || !(1..=MAX_START_PAGE).contains(&first) {
        return Err(PreviewError::Invalid);
    }
    let _permit = PREVIEWS.try_acquire().map_err(|_| PreviewError::Busy)?;
    tokio::time::timeout(Duration::from_secs(5), read_text(bytes, first))
        .await
        .map_err(|_| PreviewError::Timeout)?
}

async fn read_text(bytes: &[u8], first: usize) -> Result<Preview, PreviewError> {
    let mut child = Command::new("pdftotext")
        .args([
            "-f",
            &first.to_string(),
            "-l",
            &(first + MAX_PAGES).to_string(),
            "-enc",
            "UTF-8",
            "-",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| PreviewError::Unavailable)?;
    let mut stdin = child.stdin.take().ok_or(PreviewError::Unavailable)?;
    let stdout = child.stdout.take().ok_or(PreviewError::Unavailable)?;
    let mut output = Vec::new();
    let mut stdout = stdout.take(MAX_OUTPUT + 1);

    tokio::try_join!(
        async {
            let result = stdin.write_all(bytes).await;
            drop(stdin);
            match result {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                Err(_) => Err(PreviewError::Unavailable),
            }
        },
        async {
            stdout
                .read_to_end(&mut output)
                .await
                .map_err(|_| PreviewError::Unavailable)?;
            if output.len() as u64 > MAX_OUTPUT {
                return Err(PreviewError::TooLarge);
            }
            Ok(())
        }
    )?;
    if !child
        .wait()
        .await
        .map_err(|_| PreviewError::Unavailable)?
        .success()
    {
        return Err(PreviewError::Invalid);
    }
    parse_pages(&output, first)
}

fn parse_pages(output: &[u8], first: usize) -> Result<Preview, PreviewError> {
    let text = std::str::from_utf8(output).map_err(|_| PreviewError::Invalid)?;
    let mut pages = text.split_terminator('\x0c').enumerate();
    let preview = pages
        .by_ref()
        .take(MAX_PAGES)
        .map(|(index, page)| Page {
            number: index + first,
            text: page.trim().to_owned(),
        })
        .collect();
    let truncated = pages.next().is_some();
    let next = first + MAX_PAGES;
    Ok(Preview {
        pages: preview,
        truncated,
        next_page: (truncated && next <= MAX_START_PAGE).then_some(next),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_page_numbers_even_when_the_first_page_has_no_text() {
        let result = parse_pages("\x0c a página \n\x0c".as_bytes(), 1).unwrap();
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].text, "");
        assert_eq!(result.pages[1].number, 2);
        assert_eq!(result.pages[1].text, "a página");
        assert!(!result.truncated);
    }

    #[test]
    fn reports_pages_omitted_from_the_preview() {
        let result = parse_pages("text\x0c".repeat(MAX_PAGES + 1).as_bytes(), 1).unwrap();
        assert_eq!(result.pages.len(), MAX_PAGES);
        assert!(result.truncated);
        assert_eq!(result.next_page, Some(21));
        let second = parse_pages(b"page\x0c", 21).unwrap();
        assert_eq!(second.pages[0].number, 21);
        assert_eq!(second.next_page, None);
    }
}
