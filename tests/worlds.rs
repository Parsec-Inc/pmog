//! Saved worlds: create, rename, trash, the legacy import and hosting several at once.

mod common;

use std::path::PathBuf;

use common::ports;
use pmog::app::{App, Confirm, LobbyRow, Screen};
use pmog::constants::{LEGACY_SERVER_DB, WORLD_NAME_KEY};
use pmog::db::Db;
use pmog::machine_id;
use pmog::worlds::WorldStore;

/// A scratch data folder removed on drop.
struct Base(PathBuf);

impl Base {
    fn new(tag: &str) -> Base {
        Base(std::env::temp_dir().join(format!("pmog-worlds-{tag}-{}-{}", std::process::id(), rand::random::<u32>())))
    }
}

impl Drop for Base {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn worlds_are_created_listed_renamed_and_trashed() {
    let base = Base::new("store");
    let store = WorldStore::under(&base.0);
    assert!(store.list().is_empty());
    let first = store.create("Mundo A").unwrap();
    let second = store.create("Mundo B").unwrap();
    let names: Vec<String> = store.list().into_iter().map(|w| w.name).collect();
    assert_eq!(names, ["Mundo A", "Mundo B"], "oldest first");

    store.rename(&first, "Mundo Renomeado").unwrap();
    assert_eq!(store.list()[0].name, "Mundo Renomeado");

    store.trash(&second).unwrap();
    assert_eq!(store.list().len(), 1);
    assert!(store.trash.join(second.path.file_name().unwrap()).exists(), "deleted worlds stay recoverable");
}

#[test]
fn the_old_single_save_becomes_the_first_world() {
    let base = Base::new("legacy");
    let store = WorldStore::under(&base.0);
    let legacy = base.0.join(LEGACY_SERVER_DB);
    {
        std::fs::create_dir_all(&base.0).unwrap();
        let db = Db::open(&legacy).unwrap();
        db.set_meta("probe", "old progress").unwrap();
    }
    let world = store.import_legacy(&base.0, "Servidor de Igor").unwrap().expect("imported");
    assert!(!legacy.exists());
    let listed = store.list();
    assert_eq!(listed, vec![world.clone()]);
    let db = Db::open(&world.path).unwrap();
    assert_eq!(db.meta("probe").unwrap().as_deref(), Some("old progress"), "same data, new place");
    assert_eq!(db.meta(WORLD_NAME_KEY).unwrap().as_deref(), Some("Servidor de Igor"));
    assert!(store.import_legacy(&base.0, "outro").unwrap().is_none(), "imports only once");
}

#[test]
fn one_computer_hosts_several_worlds_at_once() {
    let base = Base::new("app");
    let store = WorldStore::under(&base.0);
    store.create("Mundo Antigo").unwrap();
    let mut app = App::new("Igor".into(), machine_id::for_bot("worlds"), None, Some(store.clone()), ports(35));
    let saved = app.worlds[0].id;
    assert_eq!(app.lobby_rows(), vec![LobbyRow::World(saved), LobbyRow::Host]);

    // Host the saved world, leave it running, then create and host a second one.
    app.lobby_enter();
    assert_eq!(app.screen, Screen::Playing, "{:?}", app.notice);
    app.disconnect(None);
    app.lobby_cursor = app.lobby_rows().iter().position(|r| *r == LobbyRow::Host).unwrap();
    app.lobby_enter();
    app.editing_world.as_mut().unwrap().typed = "Mundo Novo".into();
    app.finish_world_edit();
    assert_eq!(app.screen, Screen::Playing, "{:?}", app.notice);
    assert_eq!(app.hosts.len(), 2, "both worlds run together");
    assert_ne!(app.hosts[0].port, app.hosts[1].port);
    assert_eq!(app.current_host().map(|h| h.name.as_str()), Some("Mundo Novo"));
    assert_eq!(store.list().len(), 2, "the new world is saved");

    // Stopping one keeps the other.
    let old_port = app.hosts[0].port;
    app.disconnect(None);
    app.lobby_cursor = 0;
    app.ask_stop_selected();
    assert_eq!(app.confirm, Some(Confirm::StopServer(old_port)));
    app.answer(true);
    assert_eq!(app.hosts.len(), 1);
    assert!(app.lobby_rows().contains(&LobbyRow::World(saved)), "a stopped world is back in the saved list");

    // Rename and delete only stopped worlds.
    app.lobby_cursor = app.lobby_rows().iter().position(|r| *r == LobbyRow::World(saved)).unwrap();
    app.start_world_rename();
    app.editing_world.as_mut().unwrap().typed = "Mundo Clássico".into();
    app.finish_world_edit();
    assert_eq!(app.world(saved).unwrap().name, "Mundo Clássico");
    app.ask_delete_world();
    assert_eq!(app.confirm, Some(Confirm::DeleteWorld(saved)));
    app.answer(true);
    assert!(app.world(saved).is_none());
    assert_eq!(store.list().len(), 1);

    app.lobby_cursor = 0;
    app.ask_delete_world();
    assert!(app.confirm.is_none(), "a running world is not deleted");
    app.shutdown();
}
