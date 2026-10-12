//! macOS menu bar: turns the [`MenuBar`] description into `NSMenu`s and puts
//! them in `NSApplication`'s main menu, then reports the user's picks back to
//! the egui side as [`Action`]s.
//!
//! AppKit owns the menu bar, so the app must not draw its own. Key equivalents
//! are registered here and consumed by AppKit before the window ever sees the
//! key event, which is why the egui keyboard map must stay in sync with
//! [`App::menu_spec`].

use std::cell::RefCell;
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, ClassType, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSControlStateValue, NSControlStateValueOff, NSControlStateValueOn,
    NSEventModifierFlags, NSMenu, NSMenuItem,
};
use objc2_foundation::{NSInteger, NSString};

use crate::app::{Action, APP_TITLE};
use crate::menu::{Item, Key, MenuBar, Shortcut};

define_class!(
    /// Receives the `menuItemInvoked:` message for every item we own.
    ///
    /// AppKit's `NSMenuItem.target` is a non-owning reference, so the object is
    /// kept alive by [`State`] for as long as the menu bar it serves.
    #[unsafe(super(NSObject))]
    pub struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(menuItemInvoked:))]
        fn menu_item_invoked(&self, item: &NSMenuItem) {
            let tag = item.tag();
            if let Some(action) = tag_lookup(tag) {
                EVENTS.lock().expect("menu event queue").push(action);
                // The pick came from AppKit, not from the window, so egui has no
                // idea it should draw another frame.
                if let Some(ctx) = CONTEXT.lock().expect("menu context").as_ref() {
                    ctx.request_repaint();
                }
            }
        }
    }
);

thread_local! {
    /// The menu bar currently installed, plus the objects AppKit refers to but
    /// does not keep alive for us.
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Picked menu items, handed over on the next frame.
static EVENTS: Mutex<Vec<Action>> = Mutex::new(Vec::new());

/// Tag index into the actions of the installed menu bar.
static TAGS: Mutex<Vec<Action>> = Mutex::new(Vec::new());

/// Handle on the UI, so a menu pick can ask for a frame.
static CONTEXT: Mutex<Option<egui::Context>> = Mutex::new(None);

pub fn attach(ctx: &egui::Context) {
    *CONTEXT.lock().expect("menu context") = Some(ctx.clone());
}

struct State {
    // Held so the objects AppKit refers to without retaining stay alive.
    #[allow(dead_code)]
    main: Retained<NSMenu>,
    #[allow(dead_code)]
    windows: Retained<NSMenu>,
    #[allow(dead_code)]
    target: Retained<MenuTarget>,
    /// The description the installed menus were built from; unchanged means
    /// there is nothing to rebuild.
    spec: MenuBar,
}

fn tag_lookup(tag: NSInteger) -> Option<Action> {
    let tags = TAGS.lock().expect("menu tag table");
    if tag < 0 {
        return None;
    }
    tags.get(tag as usize).copied()
}

pub fn sync(bar: &MenuBar) {
    let Some(mtm) = MainThreadMarker::new() else {
        // Not on the main thread: AppKit forbids touching the menu bar, and the
        // app only ever builds it from `App::draw`, which is main-thread.
        return;
    };
    if STATE.with(|state| state.borrow().as_ref().is_some_and(|s| s.spec == *bar)) {
        return;
    }

    let target: Retained<MenuTarget> = unsafe { msg_send![MenuTarget::class(), new] };
    let mut tags: Vec<Action> = Vec::new();
    let main = build(bar, mtm, &target, &mut tags);

    // AppKit appends the window list to whichever menu is the Window menu, so
    // the holder has to sit in the bar itself.
    let windows = windows_menu(mtm, &mut tags);
    let window_item = NSMenuItem::new(mtm);
    window_item.setTitle(&NSString::from_str("Window"));
    window_item.setSubmenu(Some(&windows));
    main.addItem(&window_item);

    {
        let mut table = TAGS.lock().expect("menu tag table");
        *table = tags;
    }

    let app = NSApplication::sharedApplication(mtm);
    app.setMainMenu(Some(&main));
    app.setWindowsMenu(Some(&windows));

    STATE.with(|state| {
        *state.borrow_mut() = Some(State {
            main,
            windows,
            target,
            spec: bar.clone(),
        })
    });
}

pub fn take_events() -> Vec<Action> {
    let mut queue = EVENTS.lock().expect("menu event queue");
    std::mem::take(&mut *queue)
}

fn build(
    bar: &MenuBar,
    mtm: MainThreadMarker,
    target: &Retained<MenuTarget>,
    tags: &mut Vec<Action>,
) -> Retained<NSMenu> {
    let main = root_menu(mtm);

    // The application menu: AppKit draws its title as the app name.
    let app_item = NSMenuItem::new(mtm);
    let app_menu = root_menu(mtm);
    add_items(&app_menu, &bar.app, mtm, target, tags);
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    add_standard_app_items(&app_menu, mtm, target, tags);
    app_item.setSubmenu(Some(&app_menu));
    main.addItem(&app_item);

    for (title, items) in &bar.menus {
        let holder = NSMenuItem::new(mtm);
        holder.setTitle(&NSString::from_str(title));
        let submenu = root_menu(mtm);
        add_items(&submenu, items, mtm, target, tags);
        holder.setSubmenu(Some(&submenu));
        main.addItem(&holder);
    }
    main
}

/// A top level menu whose enabling is driven by the app, not by AppKit's
/// responder-chain guesswork.
fn root_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    menu
}

fn add_items(
    menu: &NSMenu,
    items: &[Item],
    mtm: MainThreadMarker,
    target: &Retained<MenuTarget>,
    tags: &mut Vec<Action>,
) {
    for item in items {
        match item {
            Item::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
            Item::Submenu { label, items } => {
                let holder = NSMenuItem::new(mtm);
                holder.setTitle(&NSString::from_str(label));
                let submenu = root_menu(mtm);
                add_items(&submenu, items, mtm, target, tags);
                holder.setSubmenu(Some(&submenu));
                menu.addItem(&holder);
            }
            Item::Command {
                label,
                shortcut,
                action,
                enabled,
                marked,
            } => {
                if *action == Action::About {
                    // Keep the platform About panel: it uses the bundle icon,
                    // name, version and copyright from Info.plist.
                    let item = standard_item(
                        &NSString::from_str(label),
                        sel!(orderFrontStandardAboutPanel:),
                        "",
                        NSEventModifierFlags::empty(),
                        mtm,
                    );
                    item.setEnabled(*enabled);
                    menu.addItem(&item);
                    continue;
                }
                let tag = tags.len() as NSInteger;
                tags.push(*action);
                let item = command_item(label, mtm, target);
                item.setTag(tag);
                item.setEnabled(*enabled);
                item.setState(state_value(*marked));
                if let Some(shortcut) = shortcut {
                    let (key, mask) = key_equivalent(*shortcut);
                    item.setKeyEquivalent(&key);
                    item.setKeyEquivalentModifierMask(mask);
                }
                menu.addItem(&item);
            }
        }
    }
}

fn command_item(
    label: &str,
    mtm: MainThreadMarker,
    target: &Retained<MenuTarget>,
) -> Retained<NSMenuItem> {
    let title = NSString::from_str(label);
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &title,
            Some(sel!(menuItemInvoked:)),
            &NSString::from_str(""),
        )
    };
    // `target` is not retained by AppKit; `State` keeps our object alive.
    let target_ref: &AnyObject = unsafe { &*Retained::as_ptr(target).cast::<AnyObject>() };
    unsafe { item.setTarget(Some(target_ref)) };
    item
}

fn state_value(marked: bool) -> NSControlStateValue {
    if marked {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}

/// Turns a [`Shortcut`] into the character and modifier mask AppKit matches.
fn key_equivalent(shortcut: Shortcut) -> (Retained<NSString>, NSEventModifierFlags) {
    let mut mask = NSEventModifierFlags::empty();
    if shortcut.command {
        mask |= NSEventModifierFlags::Command;
    }
    if shortcut.control {
        mask |= NSEventModifierFlags::Control;
    }
    if shortcut.option {
        mask |= NSEventModifierFlags::Option;
    }
    if shortcut.shift {
        mask |= NSEventModifierFlags::Shift;
    }
    // AppKit matches the character case-insensitively but displays the glyph it
    // is given, so lowercase letters keep the menu reading like the keyboard.
    let character = match shortcut.key {
        Key::Char(c) => c.to_lowercase().to_string(),
        Key::Return => "\r".to_owned(),
        Key::Delete => "\u{8}".to_owned(),
    };
    (NSString::from_str(&character), mask)
}

/// Services, Hide, Hide Others and Show All: implemented by `NSApplication`
/// itself, so they go straight to the responder chain.
fn add_standard_app_items(
    menu: &NSMenu,
    mtm: MainThreadMarker,
    target: &Retained<MenuTarget>,
    tags: &mut Vec<Action>,
) {
    // Run from a shell the process name is the binary, "lumiere"; the bundled
    // app is "Lumiere", and the menu should read the same either way.
    let name = NSString::from_str(crate::app::APP_TITLE);

    let services_item = NSMenuItem::new(mtm);
    services_item.setTitle(&NSString::from_str("Services"));
    let services = NSMenu::new(mtm);
    services_item.setSubmenu(Some(&services));
    menu.addItem(&services_item);
    NSApplication::sharedApplication(mtm).setServicesMenu(Some(&services));

    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&standard_item(
        &NSString::from_str("Hide ").stringByAppendingString(&name),
        sel!(hide:),
        "h",
        NSEventModifierFlags::Command,
        mtm,
    ));
    menu.addItem(&standard_item(
        &NSString::from_str("Hide Others"),
        sel!(hideOtherApplications:),
        "h",
        NSEventModifierFlags::Command | NSEventModifierFlags::Option,
        mtm,
    ));
    menu.addItem(&standard_item(
        &NSString::from_str("Show All"),
        sel!(unhideAllApplications:),
        "",
        NSEventModifierFlags::empty(),
        mtm,
    ));

    menu.addItem(&NSMenuItem::separatorItem(mtm));
    let quit = Item::key(
        format!("Quit {}", APP_TITLE),
        Action::Quit,
        Shortcut::cmd(Key::Char('q')),
    );
    add_items(menu, &[quit], mtm, target, tags);
}

fn standard_item(
    title: &NSString,
    action: objc2::runtime::Sel,
    key: &str,
    mask: NSEventModifierFlags,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            title,
            Some(action),
            &NSString::from_str(key),
        )
    };
    item.setKeyEquivalentModifierMask(mask);
    item
}

/// The Window menu, handed to AppKit so it can append the window list.
fn windows_menu(mtm: MainThreadMarker, _tags: &mut Vec<Action>) -> Retained<NSMenu> {
    let menu = root_menu(mtm);
    menu.addItem(&standard_item(
        &NSString::from_str("Minimize"),
        sel!(performMiniaturize:),
        "m",
        NSEventModifierFlags::Command,
        mtm,
    ));
    menu.addItem(&standard_item(
        &NSString::from_str("Zoom"),
        sel!(performZoom:),
        "",
        NSEventModifierFlags::empty(),
        mtm,
    ));
    menu
}
