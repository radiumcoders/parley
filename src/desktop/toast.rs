//! Windows notifications: a toast shown by Windows PowerShell, which every Windows 10 and
//! 11 has. Kept apart from the Win32 code so its escaping is tested on every platform.

/// PowerShell's own app id: toasts need one that's registered with Windows.
const APP_ID: &str =
    r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

/// The arguments for powershell.exe that show a toast with `title` and `body`.
pub fn powershell_args(title: &str, body: &str) -> Vec<String> {
    let script = format!(
        "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null\n\
         [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] | Out-Null\n\
         $xml = New-Object Windows.Data.Xml.Dom.XmlDocument\n\
         $xml.LoadXml('<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>')\n\
         [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{APP_ID}').Show([Windows.UI.Notifications.ToastNotification]::new($xml))",
        xml_escape(title),
        xml_escape(body),
    );
    [
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-EncodedCommand",
        &encode(&script),
    ]
    .map(String::from)
    .to_vec()
}

/// Escaped for XML text; no `'` is left, so it also sits safely in PowerShell's '...'.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// What `-EncodedCommand` takes: the script as UTF-16LE, in base64.
fn encode(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&bytes)
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // UTF-16LE, as PowerShell decodes it.
        assert_eq!(encode("hi"), "aABpAA==");
    }

    #[test]
    fn text_cannot_break_out() {
        assert_eq!(
            xml_escape("it's <b> & \"x\""),
            "it&apos;s &lt;b&gt; &amp; &quot;x&quot;"
        );
        let args = powershell_args("parley", "Claude's asking");
        assert_eq!(args[4], "-EncodedCommand");
        assert!(!args[5].contains(' '));
    }
}
