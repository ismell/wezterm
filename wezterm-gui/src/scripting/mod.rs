use crate::frontend::try_front_end;
use crate::inputmap::InputMap;
use config::keyassignment::KeyTable;
use config::lua::get_or_create_sub_module;
use config::lua::mlua::{self, Lua};
use config::{DeferredKeyCode, GpuInfo, Key, KeyNoAction};
use luahelper::dynamic_to_lua_value;
use mux::window::WindowId as MuxWindowId;
use std::collections::HashMap;
use wezterm_dynamic::ToDynamic;
use window::WindowOps;

pub mod guiwin;

fn luaerr(err: anyhow::Error) -> mlua::Error {
    mlua::Error::external(err)
}

fn open_with<'lua>(lua: &'lua Lua, (url, app): (String, Option<String>)) -> mlua::Result<()> {
    let active_window: mlua::Value = lua.named_registry_value("wezterm-active-window")?;
    let mut handled = false;

    if let mlua::Value::UserData(ud) = active_window {
        if let Ok(gui_win) = ud.borrow::<guiwin::GuiWin>() {
            if app.is_none() {
                gui_win.window.open_url(&url, true);
                handled = true;
            }
        }
    }

    if !handled {
        if let Some(app) = app {
            wezterm_open_url::open_with(&url, &app);
        } else {
            wezterm_open_url::open_url(&url);
        }
    }
    Ok(())
}

pub fn register(lua: &Lua) -> anyhow::Result<()> {
    let window_mod = get_or_create_sub_module(lua, "gui")?;

    window_mod.set(
        "gui_window_for_mux_window",
        lua.create_async_function(|_, mux_window_id: MuxWindowId| async move {
            let fe =
                try_front_end().ok_or_else(|| mlua::Error::external("not called on gui thread"))?;
            let _ = fe.reconcile_workspace().await;
            let win = fe.gui_window_for_mux_window(mux_window_id).ok_or_else(|| {
                mlua::Error::external(format!(
                    "mux window id {mux_window_id} is not currently associated with a gui window"
                ))
            })?;
            Ok(win)
        })?,
    )?;

    fn key_table_to_lua(table: &KeyTable) -> Vec<Key> {
        let mut keys = vec![];
        for ((key, mods), entry) in table {
            keys.push(Key {
                key: KeyNoAction {
                    key: DeferredKeyCode::KeyCode(key.clone()),
                    mods: *mods,
                },
                action: entry.action.clone(),
            });
        }
        keys
    }

    window_mod.set(
        "gui_windows",
        lua.create_function(|_, _: ()| {
            let fe =
                try_front_end().ok_or_else(|| mlua::Error::external("not called on gui thread"))?;
            Ok(fe.gui_windows())
        })?,
    )?;

    window_mod.set(
        "default_keys",
        lua.create_function(|lua, _: ()| {
            let map = InputMap::default_input_map();
            let keys = key_table_to_lua(&map.keys.default);
            dynamic_to_lua_value(lua, keys.to_dynamic())
        })?,
    )?;

    window_mod.set(
        "default_key_tables",
        lua.create_function(|lua, _: ()| {
            let inputmap = InputMap::default_input_map();
            let mut tables: HashMap<String, Vec<Key>> = HashMap::new();
            for (k, table) in &inputmap.keys.by_name {
                let keys = key_table_to_lua(table);
                tables.insert(k.to_string(), keys);
            }
            dynamic_to_lua_value(lua, tables.to_dynamic())
        })?,
    )?;

    window_mod.set(
        "enumerate_gpus",
        lua.create_function(|_, _: ()| {
            let backends = wgpu::Backends::all();
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends,
                ..Default::default()
            });
            let gpus: Vec<GpuInfo> = instance
                .enumerate_adapters(backends)
                .into_iter()
                .map(|adapter| {
                    let info = adapter.get_info();
                    crate::termwindow::webgpu::adapter_info_to_gpu_info(info)
                })
                .collect();
            Ok(gpus)
        })?,
    )?;

    let wezterm_mod = config::lua::get_or_create_module(lua, "wezterm")?;
    wezterm_mod.set("open_with", lua.create_function(open_with)?)?;

    Ok(())
}
