//! macOS: files dropped on the Dock icon, opened with "Open With", or sent by
//! `open -a Retype file` arrive as `application:openURLs:` on the app delegate.
//!
//! winit owns the delegate and panics if it is replaced, so Retype adds that one
//! method to winit's delegate class at runtime instead. It must be in place
//! before AppKit finishes launching, because files from "Open With" are handed
//! over during launch, so it is added from the will-finish-launching notification.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{MainThreadMarker, ffi, msg_send, sel};
use objc2_app_kit::NSApplication;
use objc2_foundation::{NSArray, NSURL};
use std::path::PathBuf;

unsafe extern "C-unwind" fn open_urls(_this: *mut AnyObject, _cmd: Sel, _app: *mut AnyObject, urls: *mut NSArray<NSURL>) {
    let Some(urls) = (unsafe { urls.as_ref() }) else { return };
    let files: Vec<PathBuf> = urls.iter().filter_map(|u| u.path()).map(|p| PathBuf::from(p.to_string())).collect();
    if !files.is_empty() {
        crate::ipc::deliver(crate::ipc::Msg { files, target: None });
    }
}

/// Call before the event loop starts. The method is added when AppKit announces
/// it will finish launching (winit's delegate exists by then).
pub fn install_open_handler_early() {
    use objc2_app_kit::NSApplicationWillFinishLaunchingNotification;
    use objc2_foundation::NSNotificationCenter;
    let center = NSNotificationCenter::defaultCenter();
    let block = block2::RcBlock::new(|_n: std::ptr::NonNull<objc2_foundation::NSNotification>| install_open_handler());
    let token = unsafe { center.addObserverForName_object_queue_usingBlock(Some(NSApplicationWillFinishLaunchingNotification), None, None, &block) };
    std::mem::forget(token);
}

fn install_open_handler() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    let Some(delegate) = app.delegate() else { return };
    let delegate: Retained<AnyObject> = unsafe { Retained::cast_unchecked(delegate) };
    let cls: *const AnyClass = unsafe { msg_send![&*delegate, class] };
    let imp: Imp = unsafe { std::mem::transmute::<unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut NSArray<NSURL>), Imp>(open_urls) };
    // v = void, @ = self, : = _cmd, @ = NSApplication, @ = NSArray
    unsafe {
        ffi::class_addMethod(cls as *mut AnyClass, sel!(application:openURLs:), imp, c"v@:@@".as_ptr());
    }
}

/// ⌘V reaches egui only when the clipboard holds text. A copied file or image
/// has none, so watch the key directly and let the app read the clipboard itself.
pub fn watch_paste_key() {
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
    let block = block2::RcBlock::new(|ev: std::ptr::NonNull<NSEvent>| -> *mut NSEvent {
        let e = unsafe { ev.as_ref() };
        let cmd = e.modifierFlags().contains(NSEventModifierFlags::Command);
        let v = e.charactersIgnoringModifiers().is_some_and(|c| c.to_string().eq_ignore_ascii_case("v"));
        if cmd && v {
            crate::platform::note_paste_key();
        }
        ev.as_ptr()
    });
    let token = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block) };
    std::mem::forget(token);
}

/// A transparent window's shadow follows its opaque pixels but is computed once;
/// recompute it after the card changes height.
pub fn refresh_shadow() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    for w in app.windows().iter() {
        w.invalidateShadow();
    }
}

/// A trackpad tap: felt while the user's finger is on a Force Touch trackpad,
/// which it is during a drag. Does nothing on other hardware.
pub fn haptic(strong: bool) {
    use objc2_app_kit::{NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime, NSHapticFeedbackPerformer};
    let performer = NSHapticFeedbackManager::defaultPerformer();
    let pattern = if strong { NSHapticFeedbackPattern::LevelChange } else { NSHapticFeedbackPattern::Alignment };
    performer.performFeedbackPattern_performanceTime(pattern, NSHapticFeedbackPerformanceTime::Now);
}

pub fn set_dock_icon(visible: bool) {
    use objc2_app_kit::NSApplicationActivationPolicy;
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    let policy = if visible { NSApplicationActivationPolicy::Regular } else { NSApplicationActivationPolicy::Accessory };
    app.setActivationPolicy(policy);
    if visible {
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

/// The Open panel, modeless: it returns at once and hands the chosen files to the
/// app's inbox when the user clicks Open. A modal panel (`runModal`) would run a
/// nested event loop inside the frame update, and any event winit delivers during
/// it (a drag passing over the window, for one) re-enters the event loop and aborts.
pub fn choose_files() {
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
    use objc2_foundation::NSString;
    let Some(mtm) = MainThreadMarker::new() else { return };
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(true);
    panel.setAllowsMultipleSelection(true);
    panel.setMessage(Some(&NSString::from_str("Choose files or folders to convert")));
    // Above the floating card, which sits at the floating window level.
    panel.setLevel(objc2_app_kit::NSFloatingWindowLevel + 1);
    let p = panel.clone();
    let handler = block2::RcBlock::new(move |result: objc2_app_kit::NSModalResponse| {
        if result == NSModalResponseOK {
            let files: Vec<PathBuf> = p.URLs().iter().filter_map(|u| u.path()).map(|s| PathBuf::from(s.to_string())).collect();
            if !files.is_empty() {
                crate::ipc::deliver(crate::ipc::Msg { files, target: None });
            }
        }
    });
    panel.beginWithCompletionHandler(&handler);
}
