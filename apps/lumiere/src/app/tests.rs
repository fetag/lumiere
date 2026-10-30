use super::*;

fn raw_input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(1280.0, 820.0),
        )),
        focused: true,
        ..Default::default()
    }
}

/// Warm the pointer up on `pos` for one frame (egui hit-tests the previous
/// pass), then press and release there.
fn click_at(ctx: &Context, app: &mut App, pos: egui::Pos2, time: &mut f64) {
    let press = vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    for (frame, events) in [vec![egui::Event::PointerMoved(pos)], press]
        .into_iter()
        .enumerate()
    {
        let mut input = raw_input();
        input.time = Some(*time + frame as f64 * 0.4);
        input.events = events;
        let _ = ctx.run(input, |ctx| app.draw(ctx));
    }
    *time += 2.0;
}

/// Warms the pointer up on `pos` for one frame, then presses and releases
/// the secondary button there — what opens a right-click menu.
fn right_click_at(ctx: &Context, app: &mut App, pos: egui::Pos2, time: &mut f64) {
    let press = vec![
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    for (frame, events) in [vec![egui::Event::PointerMoved(pos)], press]
        .into_iter()
        .enumerate()
    {
        let mut input = raw_input();
        input.time = Some(*time + frame as f64 * 0.4);
        input.events = events;
        let _ = ctx.run(input, |ctx| app.draw(ctx));
    }
    *time += 2.0;
}

/// Two quick clicks at `pos`, inside egui's 0.3 s double-click window.
fn double_click_at(ctx: &Context, app: &mut App, pos: egui::Pos2, time: &mut f64) {
    let button = |pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for (dt, events) in [
        (0.0, vec![egui::Event::PointerMoved(pos), button(true)]),
        (0.05, vec![button(false)]),
        (0.15, vec![button(true)]),
        (0.2, vec![button(false)]),
    ] {
        let mut input = raw_input();
        input.time = Some(*time + dt);
        input.events = events;
        let _ = ctx.run(input, |ctx| app.draw(ctx));
    }
    *time += 1.0;
}

/// One frame carrying a trackpad scroll: pixel deltas, no modifiers, the
/// way `egui-winit` forwards a two-finger swipe.
fn scroll_frame(ctx: &Context, app: &mut App, time: &mut f64, delta: egui::Vec2) {
    let mut input = raw_input();
    input.time = Some(*time);
    input.events = vec![egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        modifiers: egui::Modifiers::NONE,
    }];
    *time += 0.1;
    let _ = ctx.run(input, |ctx| app.draw(ctx));
}

/// Paints frames until the viewer's full-size texture has arrived; the
/// viewer's gestures only work once the picture is on screen.
fn wait_for_full_image(ctx: &Context, app: &mut App, time: &mut f64) {
    let current = app.current_image_path().expect("current image");
    for _ in 0..120 {
        let mut input = raw_input();
        input.time = Some(*time);
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        *time += 0.1;
        if app.caches.full(&current).is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("the image never reached the full cache");
}

/// Runs one frame with the given pointer events, for pointer choreography
/// like a drag-out gesture.
fn drag_frame(ctx: &Context, app: &mut App, time: &mut f64, events: Vec<egui::Event>) {
    let mut input = raw_input();
    input.time = Some(*time);
    input.events = events;
    let _ = ctx.run(input, |ctx| app.draw(ctx));
    *time += 0.2;
}

/// Paints frames until `label` is laid out and returns its center.
fn labeled_center(ctx: &Context, app: &mut App, label: &str, time: &mut f64) -> Option<egui::Pos2> {
    for _ in 0..20 {
        settle(ctx, app, time);
        if let Some(rect) = theme::rect_of(ctx, label) {
            return Some(rect.center());
        }
    }
    None
}

fn click_label(ctx: &Context, app: &mut App, label: &str, time: &mut f64) -> bool {
    let Some(rect) = theme::rect_of(ctx, label) else {
        return false;
    };
    click_at(ctx, app, rect.center(), time);
    true
}

/// Like [`click_label`], but returns the output of the click's frame so a
/// test can inspect the commands that frame emitted (egui hands the
/// platform output back to the caller and clears it from the context).
fn click_label_output(
    ctx: &Context,
    app: &mut App,
    label: &str,
    time: &mut f64,
) -> Option<egui::FullOutput> {
    let rect = theme::rect_of(ctx, label)?;
    let pos = rect.center();
    let move_frame = vec![egui::Event::PointerMoved(pos)];
    let press = vec![
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ];
    let mut output = None;
    for (frame, events) in [move_frame, press].into_iter().enumerate() {
        let mut input = raw_input();
        input.time = Some(*time + frame as f64 * 0.4);
        input.events = events;
        output = Some(ctx.run(input, |ctx| app.draw(ctx)));
    }
    *time += 2.0;
    output
}

/// True if `output` — the return value of the frame that performed the
/// copy — carries a copy command with `text`. egui takes the frame's
/// output out of the context at the end of the pass, so it can only be
/// read from the `FullOutput` the frame returned. In a real build
/// egui-winit executes the command on the OS clipboard.
fn copied(output: &egui::FullOutput, text: &str) -> bool {
    output
        .platform_output
        .commands
        .iter()
        .any(|command| matches!(command, egui::OutputCommand::CopyText(t) if t == text))
}

/// Paints frames until the given text shows up (windows need a sizing pass
/// before their content is drawn).
fn painted_once(ctx: &Context, app: &mut App, needle: &str) -> String {
    let mut text = String::new();
    for _ in 0..40 {
        let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
        text = painted_text(&output.shapes);
        if text.contains(needle) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    text
}

fn settle(ctx: &Context, app: &mut App, time: &mut f64) {
    // The registry must only hold controls that are on screen right now,
    // otherwise a closed menu leaves a stale rect behind.
    theme::clear_rects(ctx);
    let mut input = raw_input();
    input.time = Some(*time);
    let _ = ctx.run(input, |ctx| app.draw(ctx));
    *time += 0.4;
}

/// Looks a command up in the menu description exactly as a menu back end
/// does, and returns what clicking it would do.
fn menu_action(app: &App, path: &[&str]) -> Action {
    let spec = app.menu_spec();
    let mut items: &[Item] = match path[0] {
        "App" => &spec.app,
        title => {
            &spec
                .menus
                .iter()
                .find(|(name, _)| name == title)
                .unwrap_or_else(|| panic!("no {title} menu"))
                .1
        }
    };
    for title in &path[1..path.len() - 1] {
        items = match items
            .iter()
            .find(|item| matches!(item, Item::Submenu { label, .. } if label == title))
        {
            Some(Item::Submenu {
                items: children, ..
            }) => children,
            Some(_) => panic!("{title} is a command, not a submenu"),
            None => panic!("no {title} submenu"),
        };
    }
    let title = path.last().unwrap();
    match items.iter().find(|item| match item {
        Item::Command { label, .. } | Item::Submenu { label, .. } => label == title,
        Item::Separator => false,
    }) {
        Some(Item::Command {
            action, enabled, ..
        }) => {
            assert!(*enabled, "{title} is disabled");
            *action
        }
        Some(_) => panic!("{title} is a submenu, not a command"),
        None => panic!("no {title} menu item"),
    }
}

/// The menu bar has to read like every other macOS app: the verbs and the
/// `\u{2318}` spellings a user already knows, and none of the Windows-era
/// habits this app shipped with.
#[test]
fn menu_uses_macos_names_and_shortcuts() {
    let ctx = egui::Context::default();
    let app = App::build(&ctx, Some(fixture_dir("menuspec")));

    let spec = app.menu_spec();
    let titles: Vec<&str> = spec.menus.iter().map(|(title, _)| title.as_str()).collect();
    assert_eq!(titles, ["File", "Edit", "View", "Go"]);

    let app_menu: Vec<&str> = spec
        .app
        .iter()
        .map(|item| match item {
            Item::Command { label, .. } | Item::Submenu { label, .. } => label.as_str(),
            Item::Separator => "",
        })
        .collect();
    assert_eq!(app_menu, ["About Lumiere", "", "Settings\u{2026}"]);

    let named = |path: &[&str]| -> String {
        let items = match path[0] {
            "App" => &spec.app,
            title => &spec.menus.iter().find(|(name, _)| name == title).unwrap().1,
        };
        for item in items {
            if let Item::Command {
                label, shortcut, ..
            } = item
            {
                if label == path[1] {
                    return shortcut
                        .map(|shortcut| shortcut.display())
                        .unwrap_or_default();
                }
            }
        }
        panic!("no {} menu item", path[1]);
    };
    assert_eq!(named(&["App", "Settings\u{2026}"]), "\u{2318},");
    assert_eq!(named(&["File", "Open\u{2026}"]), "\u{2318}O");
    assert_eq!(named(&["File", "Get Info"]), "\u{2318}I");
    assert_eq!(named(&["File", "Move to Trash"]), "\u{232B}");
    assert_eq!(named(&["View", "Reload"]), "\u{2318}R");
    assert_eq!(named(&["View", "Enter Full Screen"]), "\u{2303}\u{2318}F");
    assert_eq!(named(&["Go", "Up One Level"]), "\u{21E7}\u{2318}U");

    // The Windows-era vocabulary must be gone from every level.
    let banned = [
        "Exit",
        "Options",
        "Properties",
        "Delete",
        "Refresh",
        "Explore Folder",
        "Tools",
        "Folder Tree",
        "Preview Pane",
    ];
    fn labels(items: &[Item], out: &mut Vec<String>) {
        for item in items {
            match item {
                Item::Command { label, .. } | Item::Submenu { label, .. } => {
                    out.push(label.clone())
                }
                Item::Separator => {}
            }
        }
    }
    let mut seen = Vec::new();
    labels(&spec.app, &mut seen);
    for (_, items) in &spec.menus {
        labels(items, &mut seen);
    }
    for word in banned {
        assert!(
            !seen.iter().any(|label| label == word),
            "{word} is still in the menu bar: {seen:?}"
        );
    }
}

/// A menu entry and the key that triggers it come from the same
/// [`Shortcut`], so the two cannot drift apart.
#[test]
fn menu_shortcuts_are_the_keyboard_map() {
    let dir = fixture_dir("shortcutmap");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let _ = ctx.run(raw_input(), |ctx| app.draw(ctx));

    let cases: &[(&[&str], egui::Key, egui::Modifiers)] = &[
        (
            &["Edit", "Rotate Left"],
            egui::Key::L,
            egui::Modifiers::SHIFT | egui::Modifiers::COMMAND,
        ),
        (
            &["Edit", "Rotate Right"],
            egui::Key::R,
            egui::Modifiers::SHIFT | egui::Modifiers::COMMAND,
        ),
        (
            &["File", "Move to Trash"],
            egui::Key::Backspace,
            egui::Modifiers::NONE,
        ),
        (
            &["View", "Show Sidebar"],
            egui::Key::S,
            egui::Modifiers::COMMAND | egui::Modifiers::CTRL,
        ),
        (
            &["Go", "Pictures"],
            egui::Key::P,
            egui::Modifiers::SHIFT | egui::Modifiers::COMMAND,
        ),
    ];
    for (path, key, modifiers) in cases {
        let before = (
            app.rotation,
            app.confirm_trash.clone(),
            app.show_sidebar,
            app.dir.clone(),
        );
        let mut input = raw_input();
        input.events.push(egui::Event::Key {
            key: *key,
            pressed: true,
            modifiers: *modifiers,
            physical_key: None,
            repeat: false,
        });
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        let after = (
            app.rotation,
            app.confirm_trash.clone(),
            app.show_sidebar,
            app.dir.clone(),
        );
        assert_ne!(before, after, "{path:?} did nothing");

        // And the menu advertises the very same combination.
        let action = menu_action(&app, path);
        let declared = app
            .shortcuts()
            .into_iter()
            .find(|(_, declared)| *declared == action)
            .map(|(shortcut, _)| (shortcut.key, shortcut))
            .expect("the shortcut is not declared in the menu bar");
        assert_eq!(
            egui_modifiers(declared.1),
            *modifiers,
            "{path:?} is advertised with different modifiers"
        );
        assert!(
            egui_keys(declared.0).contains(key),
            "{path:?} is advertised on a different key"
        );
    }

    std::fs::remove_dir_all(&dir).unwrap();
}

/// The grid is painted, not built from widgets, so without an explicit
/// accessible name a screen reader hears nothing at all.
#[test]
fn thumbnails_are_readable_by_accessibility() {
    let dir = fixture_dir("a11ygrid");
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let frame = ctx.run(raw_input(), |ctx| app.draw(ctx));

    // The tree is handed to the platform in the frame's output, so this is
    // what a screen reader would have been told.
    let update = frame
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit is enabled");
    let names: Vec<String> = update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label().map(str::to_owned))
        .collect();
    for name in ["one.png", "two.png", "three.png"] {
        assert!(
            names.iter().any(|read| read.starts_with(name)),
            "{name} is not announced; the tree says {names:?}"
        );
    }

    // A cell reads like a list item: named, with the type and size in the
    // value, and carrying the selection state.
    let find = |name: &str| -> &egui::accesskit::Node {
        update
            .nodes
            .iter()
            .find(|(_, node)| {
                node.label() == Some(name) && node.role() == egui::accesskit::Role::ListItem
            })
            .map(|(_, node)| node)
            .unwrap_or_else(|| panic!("no list item labelled {name}; tree says {names:?}"))
    };
    let folder = find("nested");
    assert_eq!(folder.role_description(), Some("folder"));
    assert_eq!(folder.value(), Some("Folder"));
    assert_eq!(folder.is_selected(), Some(true), "the selected entry");

    let image = find("one.png");
    assert_eq!(image.role_description(), Some("image"));
    assert!(
        image.value().is_some_and(|value| value.starts_with("PNG")),
        "the type and size should be in the value: {:?}",
        image.value()
    );
    assert_eq!(image.is_selected(), Some(false));

    // The toolbar buttons read as buttons, not one nameless canvas.
    let refresh = update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.label() == Some("Refresh") && node.role() == egui::accesskit::Role::Button
        })
        .map(|(_, node)| node)
        .expect("the toolbar button is missing from the tree");
    assert!(refresh.supports_action(egui::accesskit::Action::Click));
    assert!(refresh.supports_action(egui::accesskit::Action::Focus));

    std::fs::remove_dir_all(&dir).unwrap();
}

/// With a cell focused, the arrow keys move the selection and the focus
/// ring together and Enter opens it, so the grid is fully operable
/// without a pointer.
#[test]
fn arrow_keys_and_enter_operate_the_focused_grid() {
    let dir = fixture_dir("a11ykeys");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    let mut time = 1.0;
    let mut frame = |app: &mut App, events: Vec<egui::Event>| {
        time += 0.4;
        let mut input = raw_input();
        input.time = Some(time);
        input.events = events;
        ctx.run(input, |ctx| app.draw(ctx))
    };
    let key = |key: egui::Key| egui::Event::Key {
        key,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
        repeat: false,
        physical_key: None,
    };

    // Lay the cells out once so they can receive focus.
    let output = frame(&mut app, vec![]);
    let ring_color = ctx.style().visuals.hyperlink_color;
    // The ring is a stroked rect; scan the frame's shapes for it.
    let ring_is_drawn = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke.color == ring_color))
    };
    assert!(!ring_is_drawn(&output), "nothing is focused yet");

    // Focus the first cell, as a Tab press or a screen reader would.
    ctx.memory_mut(|m| m.request_focus(crate::browser::grid_cell_id(0)));
    let output = frame(&mut app, vec![]);
    assert!(
        ring_is_drawn(&output),
        "the focused cell should draw the ring"
    );

    // ArrowRight moves the selection and the focus ring together.
    frame(&mut app, vec![key(egui::Key::ArrowRight)]);
    assert_eq!(app.selected, 1);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(crate::browser::grid_cell_id(1)),
        "the focus ring must follow the selection"
    );

    // ArrowDown jumps a whole row; the fixture's four entries share one
    // row, so it clamps to the last one.
    frame(&mut app, vec![key(egui::Key::ArrowDown)]);
    assert_eq!(app.selected, 3);

    // Enter opens what is selected.
    frame(&mut app, vec![key(egui::Key::Enter)]);
    assert_eq!(app.mode, Mode::Viewer);

    std::fs::remove_dir_all(&dir).unwrap();
}

/// The sidebar is driven the same way as the grid: the arrow keys move
/// the focused row and Enter navigates to it.
#[test]
fn arrow_keys_operate_the_sidebar() {
    let dir = fixture_dir("a11ysidebar");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    let mut time = 1.0;
    let mut frame = |app: &mut App, events: Vec<egui::Event>| {
        time += 0.4;
        let mut input = raw_input();
        input.time = Some(time);
        input.events = events;
        ctx.run(input, |ctx| app.draw(ctx))
    };
    let key = |key: egui::Key| egui::Event::Key {
        key,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
        repeat: false,
        physical_key: None,
    };

    frame(&mut app, vec![]);
    ctx.memory_mut(|m| m.request_focus(crate::browser::sidebar_row_id(0)));
    frame(&mut app, vec![]);
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(crate::browser::sidebar_row_id(0))
    );

    // ArrowDown moves the row and the focus together.
    frame(&mut app, vec![key(egui::Key::ArrowDown)]);
    assert_eq!(app.sidebar_selected, Some(1));
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(crate::browser::sidebar_row_id(1))
    );

    // ArrowUp walks back.
    frame(&mut app, vec![key(egui::Key::ArrowUp)]);
    assert_eq!(app.sidebar_selected, Some(0));
    assert_eq!(
        ctx.memory(|m| m.focused()),
        Some(crate::browser::sidebar_row_id(0))
    );

    // Enter navigates to the focused row: the first row is Home.
    frame(&mut app, vec![key(egui::Key::Enter)]);
    assert_eq!(app.dir, dirs::home_dir().unwrap());

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn sort_menu_reorders_the_grid() {
    let dir = std::env::temp_dir().join("lumiere_ui_test_sortmenu");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.png"), vec![b'x'; 10]).unwrap();
    std::fs::write(dir.join("b.png"), vec![b'x'; 900]).unwrap();
    std::fs::write(dir.join("c.png"), vec![b'x'; 100]).unwrap();

    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    app.act(menu_action(&app, &["View", "Sort", "by Size"]), &ctx);
    assert_eq!(app.sort, SortKey::Size);
    let names: Vec<String> = app.entries.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, ["a.png", "c.png", "b.png"]);

    app.act(menu_action(&app, &["View", "Sort", "Reverse order"]), &ctx);
    let names: Vec<String> = app.entries.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, ["b.png", "c.png", "a.png"]);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn view_menu_toggles_panels_and_thumb_size() {
    let dir = fixture_dir("viewmenu");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    assert!(app.show_sidebar && app.thumb_size == 120);
    app.act(menu_action(&app, &["View", "Show Sidebar"]), &ctx);
    assert!(!app.show_sidebar);

    app.act(menu_action(&app, &["View", "Small Thumbnails"]), &ctx);
    assert_eq!(app.thumb_size, 64);

    for path in [
        &["View", "Show Inspector"][..],
        &["View", "Show Toolbar"][..],
    ] {
        app.act(menu_action(&app, path), &ctx);
    }
    assert!(!app.show_inspector && !app.show_toolbar);

    // The status bar is off by default (feedback rides the floating
    // notice), so the menu item turns it on.
    assert!(!app.show_status);
    app.act(menu_action(&app, &["View", "Show Status Bar"]), &ctx);
    assert!(app.show_status);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn sidebar_navigates_to_the_selected_place() {
    let dir = fixture_dir("places");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;

    settle(&ctx, &mut app, &mut time);
    // The volume-root row is named per platform; navigate to whichever it is.
    let root = crate::browser::volume_root();
    assert!(click_label(
        &ctx,
        &mut app,
        &format!("sidebar:{}", root.label),
        &mut time
    ));
    assert_eq!(app.dir, root.path);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn clicking_a_sidebar_folder_navigates_to_it() {
    let home = dirs::home_dir().expect("home dir");
    let dir = fixture_dir("sidebarnav");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(home.clone()));
    let mut time = 1.0;

    // Move out of home; the visit lands in the sidebar's Recents.
    app.set_dir(dir.clone());
    assert!(app.recent_dirs.contains(&dir), "a visit becomes a recent");

    settle(&ctx, &mut app, &mut time);
    assert!(click_label(&ctx, &mut app, "sidebar:Home", &mut time));
    assert_eq!(app.dir, home);
    assert!(app.sidebar_selected.is_some());

    settle(&ctx, &mut app, &mut time);
    let fixture_name = dir.file_name().unwrap().to_string_lossy().to_string();
    assert!(click_label(
        &ctx,
        &mut app,
        &format!("sidebar:{fixture_name}"),
        &mut time
    ));
    assert_eq!(app.dir, dir);
}

/// The sidebar lights at most one row at a time: the first row, in
/// reading order, of the exact current location — never an ancestor
/// (every path starts with `/`, so the disk root would light
/// everywhere otherwise), and never twice when the folder shows up in
/// more than one section (Favorites and Recents).
#[test]
fn sidebar_lights_at_most_one_row() {
    let dir = fixture_dir("sidebarlight");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;
    let home = dirs::home_dir().expect("home dir");

    // A lit row paints its selection tint: a 5px-rounded fill clipped
    // to the 200px-wide sidebar panel (the toolbar toggles share the
    // tint but are clipped to the toolbar and round at 2px).
    let selection_bg = ctx.style().visuals.selection.bg_fill;
    let lit_rows = |shapes: &[egui::epaint::ClippedShape]| {
        shapes
            .iter()
            .filter(|clipped| {
                let egui::Shape::Rect(shape) = &clipped.shape else {
                    return false;
                };
                clipped.clip_rect.max.x <= 200.0
                    && shape.corner_radius.nw == 5
                    && shape.fill == selection_bg
            })
            .count()
    };
    let mut frame = |app: &mut App| {
        time += 0.4;
        let mut input = raw_input();
        input.time = Some(time);
        ctx.run(input, |ctx| app.draw(ctx))
    };

    // In the fixture folder no sidebar row is the current location.
    for _ in 0..3 {
        frame(&mut app);
    }
    let output = frame(&mut app);
    assert_eq!(
        lit_rows(&output.shapes),
        0,
        "nothing lights in the fixture dir"
    );

    // Home now sits in Favorites *and* Recents: only one row lights.
    app.set_dir(home.clone());
    let output = frame(&mut app);
    assert_eq!(
        lit_rows(&output.shapes),
        1,
        "one row lights for the duplicated location"
    );

    // Inside Home but at a folder that is only a recent: the recent's
    // row lights, Home's (an ancestor now) does not.
    app.set_dir(home.join("Library"));
    let output = frame(&mut app);
    assert_eq!(
        lit_rows(&output.shapes),
        1,
        "only the exact location's row lights, not its ancestors"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn files_filter_applies_from_the_filter_popover() {
    let dir = std::env::temp_dir().join("lumiere_ui_test_filter");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("one.png"), vec![b'x'; 10]).unwrap();
    std::fs::write(dir.join("two.jpg"), vec![b'x'; 10]).unwrap();

    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;

    settle(&ctx, &mut app, &mut time);
    // The wildcard filter no longer stands in the address row; it opens
    // behind the button.
    assert!(click_label(&ctx, &mut app, "Filter", &mut time));
    assert!(app.filter_open);

    // Focus the popover's field, then type the pattern.
    settle(&ctx, &mut app, &mut time);
    assert!(click_label(&ctx, &mut app, "Files filter", &mut time));

    let mut input = raw_input();
    input.time = Some(time + 0.4);
    time += 0.4;
    input.events = vec![
        egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        },
        egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        },
        egui::Event::Text("*.jpg".to_owned()),
    ];
    let _ = ctx.run(input, |ctx| app.draw(ctx));

    settle(&ctx, &mut app, &mut time);
    assert!(click_label(&ctx, &mut app, "Apply", &mut time));

    assert_eq!(app.filter, "*.jpg");
    assert!(!app.filter_open, "Apply closes the popover");
    let names: Vec<String> = app.entries.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, ["two.jpg"]);

    std::fs::remove_dir_all(&dir).unwrap();
}

/// In a narrow window the content pane takes priority: both side panels
/// step aside (their on/off state is kept, so they come back when the
/// window widens again), and the grid keeps drawing.
#[test]
fn narrow_window_keeps_the_content_pane_usable() {
    let dir = fixture_dir("narrow");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;

    let mut frame_at = |app: &mut App, width: f32| {
        time += 0.4;
        // Only the controls painted by *this* frame may be looked up.
        theme::clear_rects(&ctx);
        let mut input = raw_input();
        input.screen_rect = Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(width, 620.0),
        ));
        input.time = Some(time);
        ctx.run(input, |ctx| app.draw(ctx))
    };

    // A wide window shows both side panes...
    let mut output = frame_at(&mut app, 1280.0);
    for _ in 0..3 {
        output = frame_at(&mut app, 1280.0);
    }
    assert!(theme::rect_of(&ctx, "sidebar:Home").is_some());
    assert!(painted_text(&output.shapes).contains("Path"));

    // A narrow window hides both; the grid still draws.
    let output = frame_at(&mut app, 600.0);
    assert!(theme::rect_of(&ctx, "sidebar:Home").is_none());
    assert!(!painted_text(&output.shapes).contains("Path"));
    assert!(painted_text(&output.shapes).contains("one.png"));

    // The toggle state is untouched, so widening brings the panes back.
    assert!(app.show_sidebar && app.show_inspector);
    let output = frame_at(&mut app, 1280.0);
    assert!(theme::rect_of(&ctx, "sidebar:Home").is_some());
    assert!(painted_text(&output.shapes).contains("Path"));

    std::fs::remove_dir_all(&dir).unwrap();
}

/// A folder's right-click menu pins it into the sidebar's Locations
/// section; the pinned row offers the unpin from its own right-click
/// menu and navigates like any other.
#[test]
fn context_menu_pins_a_folder_to_the_sidebar() {
    let dir = fixture_dir("pin");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;
    let nested = dir.join("nested");

    // The first cell is the `nested` folder.
    let Some(pos) = labeled_center(&ctx, &mut app, "cell:0", &mut time) else {
        panic!("grid cell never laid out");
    };
    right_click_at(&ctx, &mut app, pos, &mut time);
    settle(&ctx, &mut app, &mut time);
    assert!(click_label(&ctx, &mut app, "Add to Sidebar", &mut time));
    assert!(app.sidebar_dirs.contains(&nested));
    assert!(app.state_dirty, "the pin must survive a restart");

    // The pinned row offers the unpin from its own right-click menu.
    // (This must happen before `nested` is visited: a visit also lands
    // it in Recents, where the row would share the same label.)
    let Some(pos) = labeled_center(&ctx, &mut app, "sidebar:nested", &mut time) else {
        panic!("pinned row never laid out");
    };
    right_click_at(&ctx, &mut app, pos, &mut time);
    settle(&ctx, &mut app, &mut time);
    assert!(click_label(
        &ctx,
        &mut app,
        "Remove from Sidebar",
        &mut time
    ));
    assert!(app.sidebar_dirs.is_empty());

    // Pin it again and follow the row: it navigates like any other.
    let Some(pos) = labeled_center(&ctx, &mut app, "cell:0", &mut time) else {
        panic!("grid cell never laid out");
    };
    right_click_at(&ctx, &mut app, pos, &mut time);
    settle(&ctx, &mut app, &mut time);
    assert!(click_label(&ctx, &mut app, "Add to Sidebar", &mut time));
    settle(&ctx, &mut app, &mut time);
    assert!(click_label(&ctx, &mut app, "sidebar:nested", &mut time));
    assert_eq!(app.dir, nested);

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Visits land in the sidebar's Recents list: newest first, no
/// duplicates, capped.
#[test]
fn recents_track_visits_newest_first() {
    let base = std::env::temp_dir().join("lumiere_ui_test_recents");
    let _ = std::fs::remove_dir_all(&base);
    let a = base.join("a");
    let b = base.join("b");
    let c = base.join("c");
    let d = base.join("d");
    let e = base.join("e");
    for p in [&a, &b, &c, &d, &e] {
        std::fs::create_dir_all(p).unwrap();
    }

    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(a.clone()));

    for p in [&b, &c, &a, &d, &e] {
        app.set_dir(p.clone());
    }
    // `a` is visited twice, so it appears once, at its newest spot.
    assert_eq!(
        app.recent_dirs,
        vec![e.clone(), d.clone(), a.clone(), c.clone(), b.clone()]
    );

    std::fs::remove_dir_all(&base).unwrap();
}

/// A restart comes back with the same sidebar / inspector state, the
/// same pinned folders and the same recent places.
#[test]
fn sidebar_and_inspector_state_survive_a_restart() {
    let dir = fixture_dir("restart");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    let nested = dir.join("nested");
    let saved = crate::state::State {
        show_sidebar: 1,
        show_inspector: 0,
        sidebar_dirs: vec![nested.clone()],
        recent_dirs: vec![nested.clone(), dir.clone()],
        ..Default::default()
    };
    apply_preferences(&mut app, &saved);

    assert!(app.show_sidebar);
    assert!(!app.show_inspector);
    assert_eq!(app.sidebar_dirs, vec![nested]);
    assert_eq!(app.recent_dirs, vec![dir.join("nested"), dir.clone()]);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn about_and_explore_dialogs_render_their_content() {
    let dir = fixture_dir("dialogs");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    app.act(Action::About, &ctx);
    let text = painted_once(&ctx, &mut app, "Version");
    assert!(text.contains("Lumiere"));
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
    assert!(!text.contains("written in Rust"));
    assert!(!text.contains("Apple ImageIO"));

    app.about_open = false;
    app.act(Action::OpenFolder, &ctx);
    let text = painted_once(&ctx, &mut app, "Explore");
    assert!(text.contains("Explore"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn viewer_keeps_the_current_image_when_the_full_cache_overflows() {
    let dir = std::env::temp_dir().join("lumiere_ui_test_fullcache");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // 5000x5000 RGBA is ~100 MB each: three of them blow the full budget.
    for name in ["a.png", "b.png", "c.png"] {
        image::RgbaImage::from_pixel(5000, 5000, image::Rgba([9, 99, 199, 255]))
            .save(dir.join(name))
            .unwrap();
    }

    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.act(Action::NextImage, &ctx);
    app.act(Action::OpenViewer, &ctx);
    let current = app.current_image_path().expect("current image");

    // A backend that can hold the 5000x5000 uploads, so the byte budget
    // (not the texture clamp) is what overflows here.
    let big_input = || {
        let mut input = raw_input();
        input.max_texture_side = Some(16384);
        input
    };

    let mut arrived = false;
    for frame in 0..120 {
        let mut input = big_input();
        input.time = Some(1.0 + frame as f64 * 0.1);
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        if app.caches.full(&current).is_some() {
            arrived = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(arrived, "current image never made it into the full cache");

    // Let the neighbour prefetches land; they overflow the budget and must
    // evict the neighbours, not the image on screen.
    for frame in 0..120 {
        let mut input = big_input();
        input.time = Some(20.0 + frame as f64 * 0.1);
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(
        app.caches.full(&current).is_some(),
        "the displayed image was evicted: the viewer would flicker"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn oversized_images_are_clamped_to_the_texture_limit() {
    let dir = std::env::temp_dir().join("lumiere_ui_test_texclamp");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    image::RgbaImage::from_pixel(3000, 3000, image::Rgba([9, 99, 199, 255]))
        .save(dir.join("big.png"))
        .unwrap();

    // Default test context advertises max_texture_side = 2048.
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.act(Action::OpenViewer, &ctx);
    let current = app.current_image_path().expect("current image");

    for frame in 0..120 {
        let mut input = raw_input();
        input.time = Some(1.0 + frame as f64 * 0.1);
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        if app.caches.full(&current).is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let entry = app.caches.full(&current).expect("clamped texture cached");
    assert_eq!((entry.width, entry.height), (3000, 3000));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn real_photo_1jpg_stays_cached() {
    let dir = PathBuf::from("/Users/james/Pictures/Photo");
    if !dir.join("1.jpg").is_file() {
        return;
    }
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let idx = app
        .entries
        .iter()
        .position(|e| e.name == "1.jpg")
        .expect("1.jpg listed");
    app.selected = idx;
    app.act(Action::OpenViewer, &ctx);
    let current = dir.join("1.jpg");

    let mut missing_after_arrival = 0;
    let mut arrived = false;
    for frame in 0..300 {
        let mut input = raw_input();
        input.time = Some(1.0 + frame as f64 * 0.1);
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        if app.caches.full(&current).is_some() {
            arrived = true;
        } else if arrived {
            missing_after_arrival += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(arrived, "1.jpg never made it into the full cache");
    assert_eq!(
        missing_after_arrival, 0,
        "1.jpg disappeared from the cache after arriving: flicker"
    );
}

#[test]
fn real_photo_huge_image_respects_decode_budget() {
    let dir = PathBuf::from("/Users/james/Pictures/Photo");
    let huge = dir.join("影像.jpg");
    if !huge.is_file() {
        return;
    }
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let idx = app
        .entries
        .iter()
        .position(|e| e.path == huge)
        .expect("影像.jpg listed");
    app.selected = idx;
    app.act(Action::OpenViewer, &ctx);

    let mut arrived = false;
    let mut frames_after_arrival = 0;
    let mut missing_after_arrival = 0;
    for frame in 0..600 {
        let mut input = raw_input();
        input.time = Some(1.0 + frame as f64 * 0.1);
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        if app.caches.full(&huge).is_some() {
            arrived = true;
            frames_after_arrival += 1;
            if frames_after_arrival > 40 {
                break;
            }
        } else if arrived {
            missing_after_arrival += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(arrived, "影像.jpg never decoded into the full cache");
    assert_eq!(
        missing_after_arrival, 0,
        "the huge image was evicted while displayed: flicker"
    );
    let entry = app.caches.full(&huge).expect("cached entry");
    assert!(entry.bytes <= MAX_FULL_BYTES, "texture over budget");
    assert_eq!((entry.width, entry.height), (10200, 14031));
}

fn fixture_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumiere_ui_test_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    for name in ["one.png", "two.png", "three.png"] {
        let img = image::RgbaImage::from_pixel(64, 48, image::Rgba([9, 99, 199, 255]));
        img.save(dir.join(name)).unwrap();
    }
    dir
}

#[test]
fn browser_viewer_and_slideshow_frames_render() {
    let dir = fixture_dir("frames");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    assert!(app.entries.iter().any(|e| e.is_dir && e.name == "nested"));
    assert_eq!(app.image_indices().len(), 3);

    for _ in 0..3 {
        let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
        assert!(!output.shapes.is_empty());
    }

    app.act(Action::NextImage, &ctx);
    assert!(app.current_image_path().is_some());
    app.act(Action::OpenViewer, &ctx);
    assert_eq!(app.mode, Mode::Viewer);
    for _ in 0..3 {
        let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
        assert!(!output.shapes.is_empty());
    }

    app.act(Action::NextImage, &ctx);
    app.act(Action::RotateCW, &ctx);
    app.act(Action::ZoomIn, &ctx);
    let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
    assert!(!output.shapes.is_empty());

    app.act(Action::Slideshow, &ctx);
    assert!(app.slideshow && app.fullscreen);
    let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
    assert!(!output.shapes.is_empty());

    app.act(Action::StopSlideshow, &ctx);
    app.act(Action::CloseViewer, &ctx);
    assert_eq!(app.mode, Mode::Browser);

    // Options / about / explore / delete confirmation windows.
    let plain = ctx.run(raw_input(), |ctx| app.draw(ctx)).shapes.len();
    app.act(Action::Settings, &ctx);
    app.act(Action::About, &ctx);
    app.act(Action::OpenFolder, &ctx);
    app.act(Action::MoveToFolder, &ctx);
    assert!(app.transfer.is_some());
    app.confirm_trash = Some(vec![dir.join("one.png")]);
    let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
    assert!(output.shapes.len() > plain);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn keyboard_navigation_moves_the_selection() {
    let dir = fixture_dir("keys");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut input = raw_input();
    input.events.push(egui::Event::Key {
        key: egui::Key::PageDown,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
        physical_key: None,
        repeat: false,
    });
    let _ = ctx.run(input, |ctx| app.draw(ctx));
    assert_eq!(app.selected, app.image_indices()[1]);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn escape_closes_open_dialogs() {
    let dir = fixture_dir("escape");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.confirm_trash = Some(vec![dir.join("one.png")]);
    app.act(Action::Settings, &ctx);
    let _ = ctx.run(raw_input(), |ctx| app.draw(ctx));
    assert!(app.confirm_trash.is_some() && app.settings_open);

    let mut input = raw_input();
    input.events.push(egui::Event::Key {
        key: egui::Key::Escape,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
        physical_key: None,
        repeat: false,
    });
    let _ = ctx.run(input, |ctx| app.draw(ctx));
    assert!(app.confirm_trash.is_none() && !app.settings_open);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn command_shortcuts_read_event_modifiers() {
    let dir = fixture_dir("cmdkeys");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut input = raw_input();
    input.events.push(egui::Event::Key {
        key: egui::Key::F,
        pressed: true,
        modifiers: egui::Modifiers::COMMAND | egui::Modifiers::CTRL,
        physical_key: None,
        repeat: false,
    });
    let _ = ctx.run(input, |ctx| app.draw(ctx));
    assert!(app.fullscreen);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn slideshow_starts_from_first_image_when_folder_selected() {
    let dir = fixture_dir("slide");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let folder = app
        .entries
        .iter()
        .position(|e| e.is_dir)
        .expect("fixture has a folder");
    app.selected = folder;
    app.act(Action::Slideshow, &ctx);
    assert!(app.slideshow);
    assert!(app.current_image_path().is_some());

    app.act(Action::StopSlideshow, &ctx);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn fullscreen_canvas_is_black() {
    let dir = fixture_dir("canvas");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    // The canvas follows the current appearance, so assert against it.
    let expected = if ctx.style().visuals.dark_mode {
        egui::Color32::from_gray(0x26)
    } else {
        egui::Color32::from_gray(0x80)
    };
    assert_eq!(canvas_color(&app, &ctx), expected);
    app.act(Action::ToggleFullscreen, &ctx);
    assert_eq!(canvas_color(&app, &ctx), egui::Color32::BLACK);
    app.act(Action::ToggleFullscreen, &ctx);
    assert_eq!(canvas_color(&app, &ctx), expected);

    std::fs::remove_dir_all(&dir).unwrap();
}

/// A pinch zooms about the gesture centre: the point under the two
/// fingers stays put, and a plain two-finger scroll must not zoom.
#[test]
fn pinch_gesture_zooms_about_the_pointer() {
    let dir = fixture_dir("pinch");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;
    app.act(Action::OpenViewer, &ctx);
    wait_for_full_image(&ctx, &mut app, &mut time);

    // In the viewer the chrome is gone, so the canvas is the whole
    // 1280x820 test window and its centre is the window's centre.
    let center = egui::pos2(640.0, 410.0);

    // Pinch in (x2) with the gesture centre on the canvas centre: the
    // zoom doubles and the pan must not move at all.
    app.fit = false;
    app.zoom = 1.0;
    app.pan = egui::Vec2::ZERO;
    let mut input = raw_input();
    input.time = Some(time);
    input.events = vec![egui::Event::PointerMoved(center), egui::Event::Zoom(2.0)];
    time += 0.1;
    let _ = ctx.run(input, |ctx| app.draw(ctx));
    assert!(!app.fit);
    assert!((app.zoom - 2.0).abs() < 0.001, "zoom was {}", app.zoom);
    assert_eq!(app.pan, egui::Vec2::ZERO, "a centred pinch must not pan");

    // Pinch out (x0.5) 200 px right of the centre: zoom 2 -> 1, and the
    // picture point under the pointer stays where it was (100 px right
    // of the image centre before and after).
    let pointer = egui::pos2(840.0, 410.0);
    let mut input = raw_input();
    input.time = Some(time);
    input.events = vec![egui::Event::PointerMoved(pointer), egui::Event::Zoom(0.5)];
    time += 0.1;
    let _ = ctx.run(input, |ctx| app.draw(ctx));
    assert!((app.zoom - 1.0).abs() < 0.001);
    assert_eq!(app.pan, egui::vec2(100.0, 0.0));

    // A pinch started from "fit" zooms relative to the fit size.
    app.fit = true;
    app.pan = egui::Vec2::ZERO;
    let fit_zoom = (1280.0_f32 / 64.0).min(820.0_f32 / 48.0);
    let mut input = raw_input();
    input.time = Some(time);
    input.events = vec![egui::Event::PointerMoved(center), egui::Event::Zoom(2.0)];
    let _ = ctx.run(input, |ctx| app.draw(ctx));
    assert!(!app.fit);
    assert!(
        (app.zoom - fit_zoom * 2.0).abs() < 0.01,
        "zoom was {}",
        app.zoom
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

/// While the picture overflows the canvas, a two-finger scroll pans it
/// one-to-one — horizontally or vertically — and never zooms or browses.
#[test]
fn scroll_pans_an_overflowing_image() {
    let dir = fixture_dir("scrollpan");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;
    app.act(Action::OpenViewer, &ctx);
    wait_for_full_image(&ctx, &mut app, &mut time);

    // Zoom far past the canvas: the 64x48 fixture now overflows it.
    app.fit = false;
    app.zoom = 40.0;
    app.pan = egui::Vec2::ZERO;
    let before = app.selected;

    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(0.0, -80.0));
    assert_eq!(app.pan, egui::vec2(0.0, 80.0), "vertical scroll pans 1:1");
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(120.0, 0.0));
    assert_eq!(
        app.pan,
        egui::vec2(-120.0, 80.0),
        "horizontal scroll pans, it must not browse"
    );

    assert_eq!(
        app.selected, before,
        "an overflowing picture cannot be swiped past"
    );
    assert_eq!(app.swipe_x, 0.0);
    assert!((app.zoom - 40.0).abs() < 0.001, "scrolling must not zoom");

    std::fs::remove_dir_all(&dir).unwrap();
}

/// While the picture fits, a horizontal swipe pages to the neighbouring
/// image: only past a real swipe distance, and one swipe — however far
/// it keeps travelling, momentum included — pages exactly once.
#[test]
fn horizontal_swipe_browses_when_the_picture_fits() {
    let dir = fixture_dir("swipe");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;
    app.act(Action::OpenViewer, &ctx);
    wait_for_full_image(&ctx, &mut app, &mut time);
    assert!(app.fit, "the viewer opens fitted");

    let images = app.image_indices();
    let first = app.selected;
    let pos = images.iter().position(|&i| i == first).unwrap();
    let next = images[(pos + 1) % images.len()];
    let after_next = images[(pos + 2) % images.len()];

    // Half a swipe builds up but does not page.
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(60.0, 0.0));
    assert_eq!(app.swipe_x, 60.0);
    assert_eq!(app.selected, first);

    // The rest of it crosses the threshold: one step forward, and the
    // gesture locks at the threshold.
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(60.0, 0.0));
    assert_eq!(app.selected, next, "a left swipe reveals the next image");
    assert_eq!(app.swipe_x, 120.0, "a paged swipe locks at the threshold");

    // The flick's momentum keeps the gesture locked: no matter how far
    // the same swipe keeps travelling, it cannot page a second time.
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(200.0, 0.0));
    assert_eq!(app.selected, next, "momentum must not page again");
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(600.0, 0.0));
    assert_eq!(app.selected, next, "one far swipe pages exactly once");

    // The tail then dies out (small per-event deltas): as soon as it
    // does, the lock releases — no silence required — and a fresh swipe
    // right after it pages again.
    for dx in [10.0, 7.0, 5.0, 4.0, 3.0] {
        scroll_frame(&ctx, &mut app, &mut time, egui::vec2(dx, 0.0));
    }
    assert_eq!(app.selected, next, "the dying tail must not page");
    assert_eq!(app.swipe_x, 0.0, "the lock releases when the tail dies");
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(60.0, 0.0));
    assert_eq!(app.selected, next);
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(60.0, 0.0));
    assert_eq!(app.selected, after_next, "the next swipe works immediately");

    // A swipe the other way pages back.
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(-130.0, 0.0));
    assert_eq!(
        app.selected, next,
        "a right swipe reveals the previous image"
    );
    assert_eq!(app.swipe_x, -120.0, "the lock flips with the page");

    // After the input went quiet (the 150 ms gap elapsed), a huge fresh
    // swipe still pages exactly once.
    std::thread::sleep(std::time::Duration::from_millis(250));
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(500.0, 0.0));
    assert_eq!(app.selected, after_next);
    assert_eq!(app.swipe_x, 120.0);
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(500.0, 0.0));
    assert_eq!(
        app.selected, after_next,
        "the rest of the same swipe is discarded"
    );

    // A vertical scroll while fitted does nothing at all.
    scroll_frame(&ctx, &mut app, &mut time, egui::vec2(0.0, -400.0));
    assert_eq!(app.selected, after_next);
    assert!(app.fit, "a vertical scroll while fitted must not zoom");

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Double click bounces between "fit" and actual size (100%).
#[test]
fn double_click_toggles_fit_and_actual_size() {
    let dir = fixture_dir("dblclick");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;
    app.act(Action::OpenViewer, &ctx);
    wait_for_full_image(&ctx, &mut app, &mut time);
    assert!(app.fit);

    // In the viewer the canvas is the whole test window; the centre is
    // clear of the floating toolbar at the bottom left.
    let center = egui::pos2(640.0, 410.0);
    double_click_at(&ctx, &mut app, center, &mut time);
    assert!(!app.fit);
    assert!(
        (app.zoom - 1.0).abs() < 0.001,
        "double click out of fit is actual size"
    );
    assert_eq!(app.pan, egui::Vec2::ZERO);

    double_click_at(&ctx, &mut app, center, &mut time);
    assert!(app.fit, "double click again goes back to fit");

    std::fs::remove_dir_all(&dir).unwrap();
}

/// A slideshow never drags the user's window around on its own: with the
/// automatic full screen off the show stays windowed, and stopping the
/// show restores the zoom, the pan and the window state from before it.
#[test]
fn slideshow_preserves_the_window_and_zoom_state() {
    let dir = fixture_dir("slideshowview");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    // The user opts out of the automatic full screen: starting and
    // stopping must not touch the window at all.
    app.slideshow_fullscreen = false;
    app.act(Action::Slideshow, &ctx);
    assert!(app.slideshow);
    assert!(
        !app.fullscreen,
        "opting out must keep a windowed show windowed"
    );

    // The user zooms while the show runs; stopping restores the
    // pre-show view (the app's defaults before the show started).
    app.fit = false;
    app.zoom = 4.0;
    app.pan = egui::vec2(30.0, -12.0);
    app.act(Action::StopSlideshow, &ctx);
    assert!(!app.slideshow && !app.fullscreen);
    assert!(app.fit);
    assert!(
        (app.zoom - 1.0).abs() < 0.001,
        "the pre-show zoom comes back"
    );
    assert_eq!(app.pan, egui::Vec2::ZERO);

    // With the automatic full screen on, the show takes the window — and
    // gives it back.
    app.slideshow_fullscreen = true;
    app.act(Action::Slideshow, &ctx);
    assert!(app.fullscreen);
    app.act(Action::StopSlideshow, &ctx);
    assert!(!app.fullscreen && !app.slideshow);

    // A show started from full screen ends full screen.
    app.act(Action::ToggleFullscreen, &ctx);
    app.act(Action::Slideshow, &ctx);
    assert!(app.fullscreen);
    app.act(Action::StopSlideshow, &ctx);
    assert!(app.fullscreen, "a full-screen user stays full screen");

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Closing the viewer mid-slideshow restores the window the show took;
/// a full screen the user chose with ⌃⌘F survives a viewer round trip.
#[test]
fn closing_the_viewer_during_a_slideshow_restores_the_window() {
    let dir = fixture_dir("closeview");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    app.slideshow_fullscreen = true;
    app.act(Action::Slideshow, &ctx);
    assert!(app.fullscreen);
    app.act(Action::CloseViewer, &ctx);
    assert_eq!(app.mode, Mode::Browser);
    assert!(!app.slideshow);
    assert!(
        !app.fullscreen,
        "the show's full screen is undone with the show"
    );

    // But a full screen the user chose themselves is not the app's to
    // undo.
    app.act(Action::ToggleFullscreen, &ctx);
    assert!(app.fullscreen);
    app.act(Action::OpenViewer, &ctx);
    assert_eq!(app.mode, Mode::Viewer);
    app.act(Action::CloseViewer, &ctx);
    assert!(
        app.fullscreen,
        "the user's own full screen survives the viewer"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn delete_action_targets_folders_too() {
    let dir = fixture_dir("del");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let folder = app
        .entries
        .iter()
        .position(|e| e.is_dir)
        .expect("fixture has a folder");
    app.selected = folder;
    app.act(Action::MoveToTrash, &ctx);
    assert_eq!(
        app.confirm_trash.as_deref(),
        Some([dir.join("nested")].as_slice())
    );

    app.confirm_trash = None;
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `⌘⌫` on a multi-selection confirms the whole selection, not just the
/// current entry, in grid order.
#[test]
fn trash_confirm_covers_the_whole_selection() {
    let dir = fixture_dir("trashmulti");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let images = app.image_indices();
    assert!(images.len() >= 2, "fixture has several images");
    let (first, last) = (images[0], *images.last().unwrap());
    app.selected = last;
    app.multi.clear();
    app.multi.insert(last);
    app.multi.insert(first);
    app.act(Action::MoveToTrash, &ctx);
    let paths = app.confirm_trash.clone().unwrap_or_default();
    assert_eq!(
        paths,
        vec![
            app.entries[first].path.clone(),
            app.entries[last].path.clone()
        ]
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Applying a trashed set removes the files and reports the result;
/// nothing is reported when every item fails.
#[test]
fn apply_trash_reports_the_result() {
    let dir = fixture_dir("trashapply");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let image = app.image_indices()[0];
    let path = app.entries[image].path.clone();
    app.selected = image;

    app.apply_trash(std::slice::from_ref(&path));
    assert!(!path.exists(), "the file must be gone");
    let notice = app.notice.clone().unwrap();
    let message = &notice.title;
    assert!(message.contains("to the Trash"), "{message}");
    assert!(
        message.contains(&crate::alerts::trash_names(std::slice::from_ref(&path))[0]),
        "{message}"
    );
    assert_ne!(app.current_image_path().as_deref(), Some(path.as_path()));

    // A file that is already gone cannot be moved: that is reported —
    // with the reason and a retry — not silently swallowed.
    app.apply_trash(std::slice::from_ref(&path));
    let notice = app.notice.clone().unwrap();
    assert!(notice.title.contains("Couldn't move"), "{}", notice.title);
    assert!(notice.detail.as_deref().is_some_and(|d| !d.is_empty()));
    assert!(
        notice
            .actions
            .iter()
            .any(|a| matches!(a, NoticeAction::Retry(_))),
        "a failed trash run must offer a retry"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn rename_rejects_paths_and_dot_names() {
    assert!(file_ops::is_single_file_name("photo.png"));
    assert!(!file_ops::is_single_file_name("nested/photo.png"));
    assert!(!file_ops::is_single_file_name("../photo.png"));
    assert!(!file_ops::is_single_file_name("."));
    assert!(!file_ops::is_single_file_name(".."));
}

#[test]
fn copy_rejects_a_destination_inside_the_source() {
    let dir = fixture_dir("copy_into_self");
    let source = dir.join("nested");
    let error =
        file_ops::copy_path(&source, &source.join("child")).expect_err("must reject recursion");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn trashing_a_viewed_image_fits_the_replacement() {
    let dir = fixture_dir("trash_viewer_fit");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.selected = app.image_indices()[0];
    let deleted = app.current_image_path().expect("image to remove");
    app.mode = Mode::Viewer;
    app.fit = false;
    app.zoom = 2.5;
    app.pan = Vec2::new(42.0, -18.0);

    std::fs::remove_file(&deleted).unwrap();
    app.report_trash(
        std::slice::from_ref(&deleted),
        OpOutcome {
            moved: 1,
            failures: Vec::new(),
        },
    );

    assert_eq!(app.mode, Mode::Viewer);
    assert_ne!(app.current_image_path().as_deref(), Some(deleted.as_path()));
    assert!(app.fit);
    assert_eq!(app.zoom, 1.0);
    assert_eq!(app.pan, Vec2::ZERO);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn trashing_in_the_browser_selects_the_next_image() {
    let dir = fixture_dir("trash_next");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let images = app.image_indices();
    assert!(images.len() >= 2, "fixture has several images");
    app.selected = images[0];
    let deleted = app.entries[images[0]].path.clone();
    let expected = app.entries[images[1]].path.clone();

    std::fs::remove_file(&deleted).unwrap();
    app.report_trash(
        std::slice::from_ref(&deleted),
        OpOutcome {
            moved: 1,
            failures: Vec::new(),
        },
    );

    assert_eq!(app.mode, Mode::Browser);
    assert_eq!(app.current_image_path(), Some(expected));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn refreshing_after_a_new_file_keeps_the_selection() {
    let dir = fixture_dir("refresh_keep");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let images = app.image_indices();
    let last = *images.last().unwrap();
    app.selected = last;
    let kept = app.entries[last].path.clone();
    let count = app.image_indices().len();

    // Sorts before every other name, so every index shifts by one.
    std::fs::copy(&kept, dir.join("0-new.png")).unwrap();
    app.refresh_listing();

    assert_eq!(app.image_indices().len(), count + 1);
    assert_eq!(app.current().map(|e| e.path.clone()), Some(kept));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_second_delete_does_not_wait_for_the_first() {
    let dir = fixture_dir("trash_twice");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let images = app.image_indices();
    assert!(images.len() >= 3, "fixture has several images");
    app.selected = images[0];
    let first = app.entries[images[0]].path.clone();
    let second = app.entries[images[1]].path.clone();
    let third = app.entries[images[2]].path.clone();

    app.trash_now(vec![first.clone()]);
    // No waiting: the picture is already gone from the list and the next one
    // is selected, before the worker has reported anything.
    assert!(app.entries.iter().all(|e| e.path != first));
    assert_eq!(app.current_image_path(), Some(second.clone()));

    app.trash_now(vec![second.clone()]);
    assert_eq!(app.current_image_path(), Some(third));

    let deadline = Instant::now() + Duration::from_secs(5);
    while (app.job.is_some() || !app.job_queue.is_empty()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        app.pump_job(&ctx);
    }
    assert!(!first.exists() && !second.exists());
    assert!(app
        .entries
        .iter()
        .all(|e| e.path != first && e.path != second));
    assert!(app.trashing.is_empty());

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn menu_shortcuts_open_dialogs() {
    let dir = fixture_dir("menukeys");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut input = raw_input();
    for key in [egui::Key::Comma, egui::Key::I] {
        input.events.push(egui::Event::Key {
            key,
            pressed: true,
            modifiers: egui::Modifiers::COMMAND,
            physical_key: None,
            repeat: false,
        });
    }
    let _ = ctx.run(input, |ctx| app.draw(ctx));
    assert!(app.settings_open);
    assert!(app.show_inspector);

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn rename_updates_disk_and_selection() {
    let dir = fixture_dir("rename");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let image = dir.join("one.png");
    app.selected = app
        .entries
        .iter()
        .position(|e| e.path == image)
        .expect("fixture has one.png");

    app.act(Action::Rename, &ctx);
    assert_eq!(app.rename_target.as_deref(), Some(image.as_path()));
    assert_eq!(app.rename_text, "one.png");

    app.apply_rename(&image, "renamed.png");
    assert!(dir.join("renamed.png").exists());
    assert!(!image.exists());
    assert_eq!(app.current().map(|e| e.name.as_str()), Some("renamed.png"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn opening_a_file_selects_it() {
    let dir = fixture_dir("openfile");
    let ctx = egui::Context::default();
    let app = App::build(&ctx, Some(dir.join("two.png")));
    assert_eq!(app.dir, dir);
    assert_eq!(app.current().map(|e| e.name.as_str()), Some("two.png"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn opening_a_document_switches_folders_and_selects_it() {
    let first = fixture_dir("open_document_first");
    let second = fixture_dir("open_document_second");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(first.clone()));

    app.open_document(second.join("three.png"));

    assert_eq!(app.dir, second);
    assert_eq!(
        app.current().map(|entry| entry.name.as_str()),
        Some("three.png")
    );
    assert!(app.scroll_to_selected);
    assert_eq!(app.mode, Mode::Viewer);

    std::fs::remove_dir_all(&first).unwrap();
    std::fs::remove_dir_all(&app.dir).unwrap();
}

#[test]
fn move_and_copy_between_folders() {
    let dir = fixture_dir("transfer");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let other = dir.join("nested");

    app.apply_transfer(&[dir.join("one.png")], &other, true);
    assert!(dir.join("one.png").exists());
    assert!(other.join("one.png").exists());

    app.apply_transfer(&[dir.join("two.png")], &other, false);
    assert!(!dir.join("two.png").exists());
    assert!(other.join("two.png").exists());

    std::fs::remove_dir_all(&dir).unwrap();
}

/// A transfer that fails names the failures, offers Show in Finder and
/// Retry, and clicking Retry really runs the operation again — on the
/// failed items only — and ends in the success card.
#[test]
fn failed_transfer_offers_retry_and_reveal() {
    let dir = fixture_dir("transferfail");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;

    // The destination does not exist yet: every item fails.
    let dest = dir.join("missing");
    app.apply_transfer(&[dir.join("one.png"), dir.join("two.png")], &dest, false);
    let notice = app.notice.clone().unwrap();
    assert!(notice.title.contains("Couldn't move"), "{}", notice.title);
    assert!(
        notice
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("Not a folder")),
        "{:?}",
        notice.detail
    );
    assert_eq!(notice.actions.len(), 2);
    assert!(notice
        .actions
        .iter()
        .any(|a| matches!(a, NoticeAction::Reveal(_))));
    assert!(notice
        .actions
        .iter()
        .any(|a| matches!(a, NoticeAction::Retry(_))));

    // Create the folder the transfer keeps failing on and click Retry:
    // the worker must move the files and end in the success card.
    std::fs::create_dir(&dest).unwrap();
    // The card's first frame is egui's sizing pass (no shapes, wrong
    // spot); from the second frame it is painted where it stays, so the
    // click needs that second frame's rect.
    settle(&ctx, &mut app, &mut time);
    settle(&ctx, &mut app, &mut time);
    assert!(
        theme::rect_of(&ctx, "Retry").is_some(),
        "the failure card must be on screen with a Retry button"
    );
    assert!(click_label(&ctx, &mut app, "Retry", &mut time));
    // Wait for the worker to finish and its outcome to be pumped; the
    // files can already be on disk before the final message arrives.
    for _ in 0..50 {
        settle(&ctx, &mut app, &mut time);
        if app.job.is_none() {
            break;
        }
    }
    assert!(
        dest.join("one.png").exists(),
        "retry must have moved one.png"
    );
    assert!(
        dest.join("two.png").exists(),
        "retry must have moved two.png"
    );
    assert!(app.job.is_none(), "the retried job must be finished");
    let notice = app.notice.clone().unwrap();
    assert!(notice.title.contains("Moved 2"), "{}", notice.title);
    assert!(notice.actions.is_empty());

    std::fs::remove_dir_all(&dir).unwrap();
}

/// A batch that half succeeds says so: the moved count, the failed item
/// named with its reason, and a retry limited to the failed item.
#[test]
fn batch_transfer_reports_each_failure() {
    let dir = fixture_dir("transfermix");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let other = dir.join("nested");

    // A folder named like the file blocks the move of that one item;
    // the other moves just fine.
    std::fs::create_dir(other.join("one.png")).unwrap();
    app.apply_transfer(&[dir.join("one.png"), dir.join("two.png")], &other, false);

    let notice = app.notice.clone().unwrap();
    assert!(notice.title.contains("Moved 1 of 2"), "{}", notice.title);
    let detail = notice.detail.unwrap();
    assert!(detail.contains("one.png"), "{detail}");
    assert!(!detail.contains("two.png"), "{detail}");
    let retry = notice
        .actions
        .iter()
        .find(|a| matches!(a, NoticeAction::Retry(_)))
        .expect("a failed batch must offer a retry");
    let NoticeAction::Retry(FileOp::Transfer { paths, .. }) = retry else {
        panic!("the retry must be a transfer");
    };
    assert_eq!(
        paths,
        &[dir.join("one.png")],
        "retry carries the failure only"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Feedback rides a floating card: it is painted while it is young and
/// goes away when the user dismisses it.
#[test]
fn notice_card_is_painted_and_dismissible() {
    let dir = fixture_dir("notice");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;

    app.notify("Cleared 3 cached thumbnail(s)");
    // The card's first frame is egui's sizing pass; it is painted from
    // the second frame on.
    settle(&ctx, &mut app, &mut time);
    time += 0.4;
    let mut input = raw_input();
    input.time = Some(time);
    let output = ctx.run(input, |ctx| app.draw(ctx));
    assert!(painted_text(&output.shapes).contains("Cleared 3 cached thumbnail(s)"));
    assert!(theme::rect_of(&ctx, "Dismiss").is_some());

    // Dismiss it: the card is gone on the next frame.
    assert!(click_label(&ctx, &mut app, "Dismiss", &mut time));
    time += 0.4;
    let mut input = raw_input();
    input.time = Some(time);
    let output = ctx.run(input, |ctx| app.draw(ctx));
    assert!(
        !painted_text(&output.shapes).contains("Cleared 3 cached thumbnail(s)"),
        "the dismissed card must be gone"
    );
    assert!(app.notice.is_none());

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn viewer_prefetches_the_neighbouring_images() {
    let dir = fixture_dir("prefetch");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.mode = Mode::Viewer;
    app.selected = app.image_indices()[0];
    app.prefetch_neighbours();

    let queued: Vec<PathBuf> = app
        .caches
        .pending
        .iter()
        .filter(|(kind, _, _)| *kind == Kind::Full)
        .map(|(_, path, _)| path.clone())
        .collect();
    assert_eq!(queued.len(), 2);
    assert!(queued.contains(&dir.join("two.png")));
    assert!(queued.contains(&dir.join("three.png")));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn grid_queues_thumbnails_wider_than_the_viewport() {
    let dir = std::env::temp_dir().join("lumiere_ui_test_prefetch_grid");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let img = image::RgbaImage::from_pixel(64, 48, image::Rgba([9, 99, 199, 255]));
    for index in 0..200 {
        img.save(dir.join(format!("img{index:03}.png"))).unwrap();
    }

    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let _ = ctx.run(raw_input(), |ctx| app.draw(ctx));

    let thumbs = app
        .caches
        .pending
        .iter()
        .filter(|(kind, _, _)| *kind == Kind::Thumb)
        .count();
    assert!(thumbs > 8, "only {thumbs} thumbnails queued");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn saved_preferences_are_restored() {
    let dir = fixture_dir("prefs");
    let path = std::env::temp_dir().join("lumiere_prefs_test.ini");
    let state = crate::state::State {
        last_dir: Some(dir.clone()),
        thumb_size: 200,
        sort: "size".to_owned(),
        sort_reverse: true,
        filter: "*.jpg".to_owned(),
        show_folders: 0,
        slideshow_fullscreen: 0,
        ..crate::state::State::default()
    };
    crate::state::save_to(&state, &path).unwrap();

    let saved = crate::state::load_from(&path);
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    apply_preferences(&mut app, &saved);

    assert_eq!(app.thumb_size, 200);
    assert_eq!(app.sort, SortKey::Size);
    assert!(app.sort_reverse);
    assert_eq!(app.filter, "*.jpg");
    assert_eq!(app.filter_edit, "*.jpg");
    assert!(!app.show_folders);
    assert!(!app.slideshow_fullscreen);

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn purged_thumbnail_work_clears_the_pending_marker() {
    let loader = Loader::new(0);
    let mut caches = Caches::default();
    let path = Path::new("/tmp/lumiere_pending.png");

    caches.request(&loader, Kind::Thumb, path, 100);
    assert_eq!(loader.queued(), 1);

    let dropped = loader.purge_thumbs();
    caches.abandon(&dropped);
    assert_eq!(loader.queued(), 0);
    assert!(caches.pending.is_empty());

    caches.request(&loader, Kind::Thumb, path, 100);
    assert_eq!(loader.queued(), 1);
    loader.shutdown();
}

#[test]
fn stale_generation_work_is_dropped_and_releasable() {
    let loader = Loader::new(0);
    let mut caches = Caches::default();

    caches.request(
        &loader,
        Kind::Preview,
        Path::new("/tmp/lumiere_stale.png"),
        100,
    );
    let (generation, dropped) = loader.bump_generation();
    assert_eq!(generation, 2);
    assert_eq!(dropped.len(), 1);

    caches.abandon(&dropped);
    assert!(caches.pending.is_empty());
    loader.shutdown();
}

fn painted_text(shapes: &[egui::epaint::ClippedShape]) -> String {
    let mut out = String::new();
    for clipped in shapes {
        if let egui::Shape::Text(text) = &clipped.shape {
            out.push_str(text.galley.text());
            out.push('\n');
        }
    }
    out
}

#[test]
fn preview_panel_lists_exif_fields() {
    let dir = std::env::temp_dir().join("lumiere_ui_test_exif");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let shot = dir.join("shot.jpg");
    imaging::write_exif_jpeg(&shot).unwrap();

    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.selected = app.image_indices()[0];

    // Only the first rows fit in the panel without scrolling.
    let text = painted_once(&ctx, &mut app, "Make");
    assert!(text.contains("Format"));
    assert!(text.contains("JPEG"));
    assert!(text.contains("LumiereTest"));
    assert!(text.contains("CAM-1"));

    let exif: std::collections::HashMap<String, String> =
        app.caches.info(&shot).exif.into_iter().collect();
    assert_eq!(
        exif.get("Date/Time").map(String::as_str),
        Some("2026-10-11 09:30:00")
    );
    assert_eq!(exif.get("Exposure").map(String::as_str), Some("1/125"));
    assert_eq!(exif.get("ISO").map(String::as_str), Some("200"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn preview_panel_lists_file_metadata() {
    let dir = fixture_dir("info");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.selected = app.image_indices()[0];

    let mut text = String::new();
    for _ in 0..40 {
        let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
        text = painted_text(&output.shapes);
        if text.contains("Format") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    // The name appears in the header, not as a grid row.
    assert!(text.contains("one.png"));
    assert!(text.contains("Type"));
    assert!(text.contains("Path"));
    assert!(text.contains("Dimensions"));
    assert!(text.contains("PNG"));

    std::fs::remove_dir_all(&dir).unwrap();
}

/// The preview pane must hold the width the user set (or the default)
/// instead of breathing with the selected entry: a long file name or
/// monospace path must not push the pane wider, and a narrow selection
/// must not shrink it. The sidebar is pinned the same way.
#[test]
fn preview_pane_holds_the_width_the_user_set() {
    let dir = fixture_dir("previewwidth");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    let width_of = |id: &str| {
        ctx.data_mut(|d| d.get_persisted::<egui::containers::panel::PanelState>(egui::Id::new(id)))
            .map(|s| s.rect.width())
    };

    let mut time = 1.0;
    let mut frame = |app: &mut App, events: Vec<egui::Event>| {
        time += 0.4;
        let mut input = raw_input();
        input.time = Some(time);
        input.events = events;
        let _ = ctx.run(input, |ctx| app.draw(ctx));
    };

    // The default width holds, even though the fixture's path is long.
    for _ in 0..4 {
        frame(&mut app, vec![]);
    }
    assert_eq!(width_of("previewpane"), Some(280.0));

    // Selecting an image (long monospace path in the metadata) does not
    // change it.
    app.selected = app.image_indices()[0];
    for _ in 0..4 {
        frame(&mut app, vec![]);
    }
    assert_eq!(width_of("previewpane"), Some(280.0));

    // ... and neither does a folder with a very long name.
    std::fs::create_dir_all(
        dir.join("a_very_long_folder_name_that_keeps_going_and_going_1234567890"),
    )
    .unwrap();
    app.rescan();
    app.selected = app
        .entries
        .iter()
        .position(|e| e.is_dir && e.name.starts_with("a_very_long"))
        .unwrap();
    for _ in 0..4 {
        frame(&mut app, vec![]);
    }
    assert_eq!(width_of("previewpane"), Some(280.0));

    // Drag the resize handle to widen the pane, as a user would. The pane
    // is 280 wide on the 1280 px test screen, so its handle sits at
    // x = 1000 (grab radius 6); press just inside the frame margin at
    // x = 1003, out from under the grid's cells. The pane keeps its right
    // edge, so the new width is |pointer.x - 1280|: dragging to x = 900
    // widens it to 380.
    let move_ptr = |x: f32| egui::Event::PointerMoved(egui::pos2(x, 400.0));
    let button = |x: f32, pressed: bool| egui::Event::PointerButton {
        pos: egui::pos2(x, 400.0),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut app, vec![move_ptr(1003.0)]); // warm the pointer up
    frame(&mut app, vec![move_ptr(1003.0), button(1003.0, true)]);
    frame(&mut app, vec![move_ptr(900.0)]); // drag out to 380 px
    frame(&mut app, vec![move_ptr(900.0), button(900.0, false)]);
    frame(&mut app, vec![]);
    assert_eq!(width_of("previewpane"), Some(380.0));

    // The dragged width holds, no matter what is selected.
    app.selected = app.image_indices()[0];
    for _ in 0..4 {
        frame(&mut app, vec![]);
    }
    assert_eq!(width_of("previewpane"), Some(380.0));

    // The sidebar holds its default width too (its full-width ScrollArea
    // already keeps the content from dictating the pane's width).
    assert_eq!(width_of("sidebar"), Some(200.0));

    std::fs::remove_dir_all(&dir).unwrap();
}

/// The Get Info toolbar button shows and hides the preview pane (the
/// Information view was merged into it): hidden, the pane's content
/// disappears from the paint output; clicking again brings it back.
#[test]
fn get_info_button_toggles_the_preview_pane() {
    let dir = fixture_dir("getinfo");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.selected = app.image_indices()[0];

    // The pane is open by default, so its metadata is painted.
    let text = painted_once(&ctx, &mut app, "Dimensions");
    assert!(text.contains("Dimensions"));

    let mut time = 1.0;

    // Click the Get Info button: the pane goes away.
    settle(&ctx, &mut app, &mut time);
    assert!(click_label(&ctx, &mut app, "Get Info", &mut time));
    assert!(!app.show_inspector);
    time += 0.4;
    let mut input = raw_input();
    input.time = Some(time);
    let output = ctx.run(input, |ctx| app.draw(ctx));
    assert!(!painted_text(&output.shapes).contains("Dimensions"));

    // Click it again: the pane is back.
    settle(&ctx, &mut app, &mut time);
    assert!(click_label(&ctx, &mut app, "Get Info", &mut time));
    assert!(app.show_inspector);
    time += 0.4;
    let mut input = raw_input();
    input.time = Some(time);
    let output = ctx.run(input, |ctx| app.draw(ctx));
    assert!(painted_text(&output.shapes).contains("Dimensions"));

    std::fs::remove_dir_all(&dir).unwrap();
}

/// The browser toolbar carries browsing controls only: the zoom / rotate
/// / flip / full-screen buttons belong to the viewer's floating toolbar,
/// the slimmed row fits an 800 px window, and every removed control is
/// still reachable through the menus (that is where it lives now).
#[test]
fn browser_toolbar_keeps_only_browser_controls() {
    let dir = fixture_dir("toolbar");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    // Paint a few frames at an 800 px wide window, as the spec's
    // acceptance width.
    let mut time = 1.0;
    for _ in 0..4 {
        time += 0.4;
        let mut input = raw_input();
        input.screen_rect = Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 0.0),
            egui::pos2(800.0, 600.0),
        ));
        input.time = Some(time);
        let _ = ctx.run(input, |ctx| app.draw(ctx));
    }

    // Browsing controls are present...
    let present: [&str; 14] = [
        "Show Sidebar",
        "Up one level",
        "Home folder",
        "Refresh",
        "Small thumbnails",
        "Medium thumbnails",
        "Large thumbnails",
        "First image",
        "Previous image",
        "Next image",
        "Last image",
        "Slideshow",
        "Move to Trash",
        "Get Info",
    ];
    for label in present {
        assert!(theme::rect_of(&ctx, label).is_some(), "{label} is missing");
    }

    // ...and the viewer-only controls are gone from the row.
    for label in [
        "Zoom in",
        "Zoom out",
        "Fit to window",
        "Actual size",
        "Rotate left",
        "Rotate right",
        "Flip horizontal",
        "Flip vertical",
        "Stop slideshow",
        "Full screen",
    ] {
        assert!(
            theme::rect_of(&ctx, label).is_none(),
            "{label} still in the row"
        );
    }

    // The whole row fits inside the 800 px window.
    let rects: Vec<_> = present
        .iter()
        .filter_map(|label| theme::rect_of(&ctx, label))
        .collect();
    let left = rects.iter().map(|r| r.min.x).fold(f32::INFINITY, f32::min);
    let right = rects
        .iter()
        .map(|r| r.max.x)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(right - left < 800.0, "the row overflows an 800 px window");

    // The removed controls are still reachable through the menus.
    assert_eq!(menu_action(&app, &["View", "Zoom In"]), Action::ZoomIn);
    assert_eq!(menu_action(&app, &["View", "Zoom Out"]), Action::ZoomOut);
    assert_eq!(
        menu_action(&app, &["View", "Fit to Window"]),
        Action::FitWindow
    );
    assert_eq!(
        menu_action(&app, &["View", "Actual Size"]),
        Action::ActualSize
    );
    assert_eq!(
        menu_action(&app, &["View", "Enter Full Screen"]),
        Action::ToggleFullscreen
    );
    assert_eq!(
        menu_action(&app, &["Edit", "Rotate Left"]),
        Action::RotateCCW
    );
    assert_eq!(
        menu_action(&app, &["Edit", "Rotate Right"]),
        Action::RotateCW
    );
    assert_eq!(
        menu_action(&app, &["Edit", "Flip Horizontal"]),
        Action::FlipH
    );
    assert_eq!(menu_action(&app, &["Edit", "Flip Vertical"]), Action::FlipV);
    assert_eq!(
        menu_action(&app, &["Go", "Play Slideshow"]),
        Action::Slideshow
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

// -- macOS file workflow: drop, right-click menu, open with, drag out --

/// Files dropped onto the window: a folder becomes the browsed
/// directory, an image from elsewhere navigates to its folder and opens
/// in the viewer, and a non-image file is ignored.
#[test]
fn dropped_paths_browse_folders_and_open_images() {
    let dir = fixture_dir("drop");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));

    // A dropped folder becomes the browsed directory.
    app.handle_dropped_paths(vec![dir.join("nested")]);
    assert_eq!(app.dir, dir.join("nested"));

    // A dropped image from another location navigates there and opens
    // the viewer on it.
    let other = std::env::temp_dir().join("lumiere_ui_test_drop_other");
    let _ = std::fs::remove_dir_all(&other);
    std::fs::create_dir_all(&other).unwrap();
    let shot = other.join("dropped.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
        .save(&shot)
        .unwrap();
    app.handle_dropped_paths(vec![shot.clone()]);
    assert_eq!(app.dir, other);
    assert_eq!(app.mode, Mode::Viewer);
    assert_eq!(app.current_image_path(), Some(shot.clone()));

    // A non-image file is ignored: the app stays where it is.
    let notes = other.join("notes.txt");
    std::fs::write(&notes, "nope").unwrap();
    app.mode = Mode::Browser;
    app.handle_dropped_paths(vec![notes]);
    assert_eq!(app.dir, other);
    assert_eq!(app.mode, Mode::Browser);

    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&other).unwrap();
}

/// The right-click menu keeps the macOS order: browse, Share, Reveal in
/// Finder, the open options, Get Info, and the destructive Move to Trash
/// last.
#[test]
fn right_click_menu_lists_the_macos_workflow_in_order() {
    let dir = fixture_dir("ctxmenu");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;

    // The first cell is the `nested` folder; `one.png` is the second.
    let Some(pos) = labeled_center(&ctx, &mut app, "cell:1", &mut time) else {
        panic!("grid cell never laid out");
    };
    right_click_at(&ctx, &mut app, pos, &mut time);
    let text = painted_once(&ctx, &mut app, "Open With");
    let lines: Vec<&str> = text.lines().collect();

    // Each menu row, in order, on its own painted line.
    let mut at = 0;
    for row in [
        "View",
        "Share",
        "Reveal in Finder",
        "Open with default app",
        "Open With",
        "Copy",
        "Copy Path",
        "Get Info",
        "Move to Trash",
    ] {
        let next = lines[at..]
            .iter()
            .position(|line| *line == row)
            .unwrap_or_else(|| panic!("{row:?} row missing"));
        at += next;
    }

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Copy / Copy Path put the file's name or its full path on the
/// clipboard, so the metadata is copyable without moving anything.
#[test]
fn copy_menu_rows_put_the_name_and_path_on_the_clipboard() {
    let dir = fixture_dir("copy");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;

    // `cell:1` is one.png.
    let Some(pos) = labeled_center(&ctx, &mut app, "cell:1", &mut time) else {
        panic!("grid cell never laid out");
    };
    right_click_at(&ctx, &mut app, pos, &mut time);
    painted_once(&ctx, &mut app, "Copy Path");
    let output =
        click_label_output(&ctx, &mut app, "Copy Path", &mut time).expect("Copy Path row laid out");
    assert!(
        copied(&output, &dir.join("one.png").display().to_string()),
        "Copy Path must put the full path on the clipboard"
    );

    // The plain Copy row copies the bare name.
    right_click_at(&ctx, &mut app, pos, &mut time);
    painted_once(&ctx, &mut app, "Copy Path");
    let output = click_label_output(&ctx, &mut app, "Copy", &mut time).expect("Copy row laid out");
    assert!(
        copied(&output, "one.png"),
        "Copy must put the file name on the clipboard"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

/// The inspector's dimensions are read from the file header, so they
/// appear on the first frame — no full decode, no "loading..." line.
#[test]
fn preview_metadata_comes_from_the_file_header() {
    let dir = fixture_dir("metahdr");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.selected = app.image_indices()[0];

    let text = painted_once(&ctx, &mut app, "64 x 48");
    assert!(text.contains("Dimensions"), "{text}");
    assert!(
        !text.contains("loading..."),
        "no decode may gate the metadata: {text}"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

/// A picture whose header no decoder will accept shows "unknown" in the
/// inspector, and its failed thumbnail is remembered instead of being
/// re-queued on every frame.
#[test]
fn a_picture_that_cannot_be_read_shows_an_error_state() {
    let dir = fixture_dir("broken");
    // A .png whose contents are not a picture at all.
    std::fs::write(dir.join("broken.png"), b"definitely not a picture").unwrap();

    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let index = app
        .entries
        .iter()
        .position(|e| e.name == "broken.png")
        .unwrap();
    app.selected = index;
    let path = dir.join("broken.png");

    // The metadata says "unknown" instead of a load state that never
    // resolves...
    let text = painted_once(&ctx, &mut app, "unknown");
    assert!(text.contains("Dimensions"), "{text}");

    // ...and the failed decode is remembered: without it the grid would
    // re-queue the broken thumbnail on every frame. The "unknown" row
    // appears immediately from the header; the loader's failure lands on
    // its own schedule, so pump frames until it does.
    let size = app.thumb_size;
    let mut time = 2.0;
    let started = Instant::now();
    while !app.caches.thumb_failed(&path, size) && started.elapsed() < Duration::from_secs(2) {
        settle(&ctx, &mut app, &mut time);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        app.caches.thumb_failed(&path, size),
        "the failed thumbnail must be noted"
    );
    assert!(
        !app.caches
            .pending
            .contains(&(Kind::Thumb, path.clone(), size)),
        "a failed thumbnail must not be re-queued"
    );
    settle(&ctx, &mut app, &mut time);
    assert!(
        !app.caches.pending.contains(&(Kind::Thumb, path, size)),
        "frames after the failure must not re-queue it either"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Right-clicking a metadata row in the inspector copies its value.
#[test]
fn copying_a_metadata_row_puts_the_value_on_the_clipboard() {
    let dir = fixture_dir("metacopy");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    app.selected = app.image_indices()[0];
    let mut time = 1.0;

    // The metadata rows settle once the preview texture lands (the well
    // shrinks to the picture's size), so wait for the rect to hold still
    // before right-clicking it.
    let mut seen = None;
    let mut rect = None;
    for _ in 0..40 {
        settle(&ctx, &mut app, &mut time);
        let this = theme::rect_of(&ctx, "meta:Dimensions");
        if seen == this {
            rect = this;
            break;
        }
        seen = this;
    }
    let Some(rect) = rect else {
        panic!("metadata row never settled");
    };
    right_click_at(&ctx, &mut app, rect.center(), &mut time);
    painted_once(&ctx, &mut app, "Copy Dimensions");
    let output = click_label_output(&ctx, &mut app, "Copy Dimensions", &mut time)
        .expect("Copy Dimensions row laid out");
    assert!(
        copied(&output, "64 x 48"),
        "the row's value must end up on the clipboard"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Open With lists applications in a submenu rather than a separate dialog.
#[test]
fn open_with_submenu_lists_applications() {
    let dir = fixture_dir("openwith");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;

    let Some(pos) = labeled_center(&ctx, &mut app, "cell:1", &mut time) else {
        panic!("grid cell never laid out");
    };
    right_click_at(&ctx, &mut app, pos, &mut time);
    painted_once(&ctx, &mut app, "Open With");
    assert!(click_label(&ctx, &mut app, "Open With", &mut time));

    let text = painted_once(&ctx, &mut app, "Preview");
    assert!(text.contains("Preview"));
    assert!(text.contains("Photos"));
    assert!(text.contains("Other…"));

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Holding the pointer on a cell and moving it past the threshold starts
/// a drag carrying the file under the press; a release before the
/// threshold is an ordinary click and starts none.
#[test]
fn drag_out_gesture_carries_the_cell_file() {
    let dir = fixture_dir("dragout");
    let ctx = egui::Context::default();
    let mut app = App::build(&ctx, Some(dir.clone()));
    let mut time = 1.0;

    let Some(start) = labeled_center(&ctx, &mut app, "cell:1", &mut time) else {
        panic!("grid cell never laid out");
    };
    app.selected = 1;
    app.multi.clear();

    let pointer = |pos: egui::Pos2, press: Option<bool>| {
        let mut events = vec![egui::Event::PointerMoved(pos)];
        if let Some(pressed) = press {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        events
    };

    // Warm the pointer up, press on the cell, then drag past the
    // threshold: the gesture fires with the file under the press.
    drag_frame(&ctx, &mut app, &mut time, pointer(start, None));
    drag_frame(&ctx, &mut app, &mut time, pointer(start, Some(true)));
    for dx in [7.0, 14.0, 21.0] {
        drag_frame(
            &ctx,
            &mut app,
            &mut time,
            pointer(egui::pos2(start.x + dx, start.y + dx / 2.0), None),
        );
    }
    assert_eq!(
        app.last_drag.as_deref(),
        Some(vec![dir.join("one.png")].as_slice())
    );

    // A second gesture released before the threshold is a click, not a
    // drag: nothing new is recorded.
    drag_frame(&ctx, &mut app, &mut time, pointer(start, Some(true)));
    drag_frame(
        &ctx,
        &mut app,
        &mut time,
        pointer(egui::pos2(start.x + 3.0, start.y), None),
    );
    drag_frame(
        &ctx,
        &mut app,
        &mut time,
        pointer(egui::pos2(start.x + 3.0, start.y), Some(false)),
    );
    assert_eq!(
        app.last_drag.as_deref(),
        Some(vec![dir.join("one.png")].as_slice())
    );

    std::fs::remove_dir_all(&dir).unwrap();
}
