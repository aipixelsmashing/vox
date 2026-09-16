//! Clipboard writes, marked private so transcripts stay out of clipboard history and cloud
//! sync, plus save/restore around synthesised pastes.
//!
//! Restore happens on *evidence that the target read the clipboard* — the field read back
//! through accessibility — never on a fixed timer. `changeCount` cannot provide that
//! evidence: a paste never moves it, only writes do (docs/TEXT-INJECTION.md#macos).

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString};
    use objc2_foundation::{NSArray, NSData, NSString};

    pub const CONCEALED_TYPE: &str = "org.nspasteboard.ConcealedType";

    /// Every item and every type, so rich clipboard contents survive a restore.
    pub struct Guard {
        items: Vec<Vec<(String, Vec<u8>)>>,
    }

    pub fn save() -> Option<Guard> {
        let pb = NSPasteboard::generalPasteboard();
        let items = pb.pasteboardItems()?;
        let mut out = Vec::new();
        for item in items.iter() {
            let mut entry = Vec::new();
            for t in item.types().iter() {
                if let Some(data) = item.dataForType(&t) {
                    entry.push((t.to_string(), data.to_vec()));
                }
            }
            out.push(entry);
        }
        Some(Guard { items: out })
    }

    impl Guard {
        pub fn restore(self) {
            let pb = NSPasteboard::generalPasteboard();
            pb.clearContents();
            if self.items.is_empty() {
                return;
            }
            let mut objects: Vec<Retained<ProtocolObject<dyn objc2_app_kit::NSPasteboardWriting>>> =
                Vec::new();
            for entry in self.items {
                let item = NSPasteboardItem::new();
                for (t, bytes) in entry {
                    let data = NSData::from_vec(bytes);
                    item.setData_forType(&data, &NSString::from_str(&t));
                }
                objects.push(ProtocolObject::from_retained(item));
            }
            let array = NSArray::from_retained_slice(&objects);
            pb.writeObjects(&array);
        }
    }

    pub fn change_count() -> isize {
        NSPasteboard::generalPasteboard().changeCount()
    }

    /// Plain text plus the concealed marker. Returns the pasteboard change count after the
    /// write, so a caller can tell whether anyone else wrote since.
    pub fn write_transcript(text: &str) -> isize {
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        // SAFETY: reading a static extern constant.
        let string_type = unsafe { NSPasteboardTypeString };
        pb.setString_forType(&NSString::from_str(text), string_type);
        pb.setString_forType(&NSString::from_str(""), &NSString::from_str(CONCEALED_TYPE));
        pb.changeCount()
    }
}

#[cfg(target_os = "macos")]
pub use mac::Guard;

/// Saves current contents and returns a guard. `None` when the pasteboard cannot be read.
#[cfg(target_os = "macos")]
pub fn save() -> Option<Guard> {
    mac::save()
}

#[cfg(target_os = "macos")]
pub fn change_count() -> isize {
    mac::change_count()
}

/// Writes the transcript, marked private. macOS: `org.nspasteboard.ConcealedType`.
/// Windows (M8): the four opt-out formats. Both are cooperative hints; the UI says so.
#[cfg(target_os = "macos")]
pub fn write_transcript(text: &str) -> Result<isize, crate::inject::Error> {
    Ok(mac::write_transcript(text))
}

/// The clipboard-only fallback path: put the text there and leave it.
pub fn set_private(text: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        mac::write_transcript(text);
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        anyhow::bail!("clipboard not implemented on this platform (M8)")
    }
}
