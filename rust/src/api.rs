//! Scripting API — Lua runtime using `mlua` crate.
//!
//! 100% Rust, no C dependencies.  Uses mlua with vendored LuaJIT/Lua5.4.

use mlua::{Function, Lua, Result, Table, Value};

/// Create a Lua VM with all TIC-80 API functions registered.
pub fn create_lua_vm() -> Result<Lua> {
    let lua = Lua::new();

    // Register TIC-80 API table
    let tic = lua.create_table()?;

    // Graphics
    tic.set("cls", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("pix", lua.create_function(|_, (x, y, c): (i32, i32, i32)| Ok::<_, mlua::Error>(0i32))?)?;
    tic.set("line", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("rect", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("rectb", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("circ", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("circb", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("elli", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("ellib", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("tri", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("trib", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("spr", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("map", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("font", lua.create_function(|_, ()| Ok(0i32))?)?;
    tic.set("print", lua.create_function(|_, ()| Ok(0i32))?)?;
    tic.set("clip", lua.create_function(|_, ()| Ok(()))?)?;

    // Input
    tic.set("btn", lua.create_function(|_, (i,): (i32,)| Ok(0u32))?)?;
    tic.set("btnp", lua.create_function(|_, (i, h, p): (i32, i32, i32)| Ok(0u32))?)?;
    tic.set("key", lua.create_function(|_, (k,): (i32,)| Ok(false))?)?;
    tic.set("keyp", lua.create_function(|_, (k, h, p): (i32, i32, i32)| Ok(false))?)?;
    tic.set("mouse", lua.create_function(|_, ()| Ok((0i32, 0i32)))?)?;

    // Memory
    tic.set("peek", lua.create_function(|_, (a, b): (i32, i32)| Ok(0u8))?)?;
    tic.set("poke", lua.create_function(|_, (a, v, b): (i32, i32, i32)| Ok(()))?)?;
    tic.set("memcpy", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("memset", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("pmem", lua.create_function(|_, (i, v, s): (i32, u32, bool)| Ok(0u32))?)?;

    // Audio
    tic.set("sfx", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("music", lua.create_function(|_, ()| Ok(()))?)?;

    // System
    tic.set("sync", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("tstamp", lua.create_function(|_, ()| Ok(0i32))?)?;
    tic.set("time", lua.create_function(|_, ()| Ok(0.0f64))?)?;
    tic.set("trace", lua.create_function(|_, (t,): (String,)| Ok(()))?)?;
    tic.set("exit", lua.create_function(|_, ()| Ok(()))?)?;
    tic.set("vbank", lua.create_function(|_, (b,): (i32,)| Ok(0i32))?)?;
    tic.set("reset", lua.create_function(|_, ()| Ok(()))?)?;

    // Flags
    tic.set("fget", lua.create_function(|_, (i, f): (i32, i32)| Ok(false))?)?;
    tic.set("fset", lua.create_function(|_, (i, f, v): (i32, i32, bool)| Ok(()))?)?;

    lua.globals().set("tic", tic)?;

    Ok(lua)
}

/// Run Lua code and return result.
pub fn run_code(lua: &Lua, code: &str) -> Result<String> {
    let result = lua.load(code).eval::<String>();
    match result {
        Ok(s) => Ok(s),
        Err(e) => Err(e),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_vm_creation() {
        let lua = create_lua_vm();
        assert!(lua.is_ok());
    }

    #[test]
    fn lua_call_cls() {
        let lua = create_lua_vm().unwrap();
        let result = lua.load(r#"tic.cls(0); return "ok""#).eval::<String>();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "ok");
    }

    #[test]
    fn lua_call_btn() {
        let lua = create_lua_vm().unwrap();
        let result = lua.load(r#"return tic.btn(0)"#).eval::<u32>();
        assert!(result.is_ok());
    }

    #[test]
    fn lua_arithmetic() {
        let lua = create_lua_vm().unwrap();
        let r: i32 = lua.load(r#"local t = tic; return 1+1"#).eval().unwrap();
        assert_eq!(r, 2);
    }

    #[test]
    fn lua_table_access() {
        let lua = create_lua_vm().unwrap();
        let r: bool = lua.load(r#"return tic ~= nil"#).eval().unwrap();
        assert!(r);
    }
}
