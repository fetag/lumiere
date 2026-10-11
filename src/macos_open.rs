//! Receive documents sent by Finder and LaunchServices.
//!
//! `open -a` and Finder's Open With do not put the selected file in argv.
//! They send a `kAEOpenDocuments` Apple event instead, including when this
//! process is already running.

#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use objc2::{define_class, msg_send, sel, ClassType, MainThreadMarker, MainThreadOnly};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Imp, Sel};
    use objc2_app_kit::{NSApplication, NSApplicationDelegateReply, NSDocument};
    use objc2_core_services::{kAEOpenDocuments, kCoreEventClass, keyDirectObject};
    use objc2_foundation::{NSArray, NSError, NSAppleEventDescriptor, NSAppleEventManager, NSURL, NSObject, NSString};

    static OPENED_FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    static EGUI_CONTEXT: OnceLock<eframe::egui::Context> = OnceLock::new();

    // AppKit tries to open a document before eframe has constructed the UI.
    // This tiny document class acknowledges it and passes the URL to Lumiere's
    // ordinary startup queue instead of letting NSDocumentController reject it.
    define_class!(
        // SAFETY: NSDocument permits subclasses that implement a reader.
        #[unsafe(super(NSDocument))]
        #[thread_kind = MainThreadOnly]
        #[name = "LumiereDocument"]
        struct LumiereDocument;

        impl LumiereDocument {
            // SAFETY: This is NSDocument's documented reader signature.
            #[unsafe(method(readFromURL:ofType:error:))]
            fn read_from_url(
                &self,
                url: &NSURL,
                _type_name: &NSString,
                _error: *mut *mut NSError,
            ) -> bool {
                if let Some(path) = url.path() {
                    queue_path(PathBuf::from(path.to_string()));
                    true
                } else {
                    false
                }
            }
        }
    );

    define_class!(
        // SAFETY: NSObject has no subclassing requirements and this class has
        // no ivars or destructor.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        struct OpenDocumentHandler;

        impl OpenDocumentHandler {
            // SAFETY: This is the signature required by NSAppleEventManager.
            #[unsafe(method(handleOpenDocuments:withReplyEvent:))]
            fn handle_open_documents(
                &self,
                event: &NSAppleEventDescriptor,
                _reply: Option<&NSAppleEventDescriptor>,
            ) {
                let Some(list) = event.paramDescriptorForKeyword(keyDirectObject) else {
                    return;
                };
                let mut opened = OPENED_FILES.lock().expect("open-file queue poisoned");
                for index in 1..=list.numberOfItems() {
                    let Some(item) = list.descriptorAtIndex(index) else {
                        continue;
                    };
                    let Some(url) = item.fileURLValue() else {
                        continue;
                    };
                    let Some(path) = url.path() else {
                        continue;
                    };
                    opened.push(PathBuf::from(path.to_string()));
                }
            }
        }
    );

    impl OpenDocumentHandler {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm);
            // SAFETY: `init` is NSObject's designated initializer.
            unsafe { msg_send![this, init] }
        }
    }

    fn queue_path(path: PathBuf) {
        OPENED_FILES
            .lock()
            .expect("open-file queue poisoned")
            .push(path);
        // An Apple event can arrive while eframe is idle. Wake it so `draw`
        // drains the queue immediately instead of waiting for user input.
        if let Some(ctx) = EGUI_CONTEXT.get() {
            ctx.request_repaint();
        }
    }

    pub fn attach_context(ctx: eframe::egui::Context) {
        let _ = EGUI_CONTEXT.set(ctx);
    }

    pub fn prepare() {
        // Calling `class` registers the Objective-C class before AppKit starts
        // dispatching the launch document event.
        let _ = LumiereDocument::class();
    }

    /// AppKit's legacy document-opening callback. Calling
    /// `replyToOpenOrPrint(Success)` is what prevents Finder from showing its
    /// generic “cannot open this format” alert after it has launched us.
    unsafe extern "C-unwind" fn application_open_files(
        _delegate: &AnyObject,
        _cmd: Sel,
        application: &NSApplication,
        filenames: &NSArray<NSString>,
    ) {
        for filename in filenames {
            queue_path(PathBuf::from(filename.to_string()));
        }
        application.replyToOpenOrPrint(NSApplicationDelegateReply::Success);
    }

    /// Finder may use this older single-file variant instead of
    /// `application:openFiles:`. Its Boolean return value is the acceptance
    /// decision behind Finder's “cannot open this format” alert.
    unsafe extern "C-unwind" fn application_open_file(
        _delegate: &AnyObject,
        _cmd: Sel,
        _application: &NSApplication,
        filename: &NSString,
    ) -> bool {
        queue_path(PathBuf::from(filename.to_string()));
        true
    }

    /// Modern LaunchServices sends URLs on current macOS releases. It does
    /// not require an explicit reply, but shares the same queue as openFiles.
    unsafe extern "C-unwind" fn application_open_urls(
        _delegate: &AnyObject,
        _cmd: Sel,
        _application: &NSApplication,
        urls: &NSArray<NSURL>,
    ) {
        for url in urls {
            if let Some(path) = url.path() {
                queue_path(PathBuf::from(path.to_string()));
            }
        }
    }

    unsafe fn add_delegate_method(
        class: *mut objc2::runtime::AnyClass,
        selector: Sel,
        implementation: Imp,
        encoding: &std::ffi::CStr,
    ) {
        let _ = unsafe { objc2::ffi::class_addMethod(class, selector, implementation, encoding.as_ptr()) };
    }

    pub fn install() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let handler = OpenDocumentHandler::new(mtm);
        let manager = NSAppleEventManager::sharedAppleEventManager();
        // SAFETY: `handleOpenDocuments:withReplyEvent:` has the signature
        // required by NSAppleEventManager, and the handler is retained below
        // for the life of the process.
        unsafe {
            manager.setEventHandler_andSelector_forEventClass_andEventID(
                &handler,
                sel!(handleOpenDocuments:withReplyEvent:),
                kCoreEventClass,
                kAEOpenDocuments,
            );
        }
        // NSAppleEventManager does not own its event handler. Leaking this
        // tiny object is intentional: it must remain valid until process exit.
        std::mem::forget(handler);

        let application = NSApplication::sharedApplication(mtm);
        let Some(delegate) = application.delegate() else {
            return;
        };
        let object = <objc2::runtime::ProtocolObject<dyn objc2_app_kit::NSApplicationDelegate> as AsRef<AnyObject>>::as_ref(&delegate);
        let class = object.class() as *const _ as *mut _;
        // SAFETY: winit owns this delegate for the process lifetime. Its
        // class does not implement either optional document-opening selector,
        // and these IMPs exactly match AppKit's declared signatures.
        unsafe {
            add_delegate_method(
                class,
                sel!(application:openFiles:),
                std::mem::transmute(application_open_files as *const ()),
                c"v@:@@",
            );
            add_delegate_method(
                class,
                sel!(application:openFile:),
                std::mem::transmute(application_open_file as *const ()),
                c"c@:@@",
            );
            add_delegate_method(
                class,
                sel!(application:openURLs:),
                std::mem::transmute(application_open_urls as *const ()),
                c"v@:@@",
            );
        }
    }

    pub fn take_opened_files() -> Vec<PathBuf> {
        std::mem::take(&mut *OPENED_FILES.lock().expect("open-file queue poisoned"))
    }
}

#[cfg(target_os = "macos")]
pub use imp::{attach_context, install, take_opened_files};

#[cfg(target_os = "macos")]
pub use imp::prepare;

#[cfg(not(target_os = "macos"))]
pub fn install() {}

#[cfg(not(target_os = "macos"))]
pub fn attach_context(_ctx: eframe::egui::Context) {}

#[cfg(not(target_os = "macos"))]
pub fn prepare() {}

#[cfg(not(target_os = "macos"))]
pub fn take_opened_files() -> Vec<PathBuf> {
    Vec::new()
}
