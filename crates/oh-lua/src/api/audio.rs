//! C++ `HexagonGame::initLua_AudioControl` (HGScripting.cpp:181-296): the
//! 11 `a_*` functions.
//!
//! `a_setMusic*` follow the C++ lambda exactly: copy the pack's music data
//! into `state.music_data`, flip `first_play`, then delegate playback to the
//! backend (C++ `stopLevelMusic`/`playLevelMusic`/`playLevelMusicAtTime`).
//! A missing music id aborts the script (C++ `assets.getMusicData` throws).

use super::{lock, State};
use crate::context::LuaContext;
use crate::error::LuaError;
use crate::metadata::FnMeta;

use crate::{add_lua_fn, add_lua_fn_try};
pub(crate) fn register(ctx: &LuaContext, state: &State) -> Result<(), LuaError> {
    let st = state.clone();
    add_lua_fn_try!(ctx,
        "a_setMusic",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |music_id: String| {
            let mut s = lock(&st);
            s.music_data = s.assets.get_music_data(&s.pack_id, &music_id)?;
            s.music_data.first_play = true;
            s.backend.stop_level_music();
            s.backend.play_level_music();
            Ok(())
        },
    )?
    .arg("musicId")
    .doc("Stop the current music and play the music with id `$0`. The id is defined in the music `.json` file, under `\"id\"`.");

    let st = state.clone();
    add_lua_fn_try!(ctx,
        "a_setMusicSegment",
        FnMeta {
            ret: "void",
            arg_types: &["std::string", "int"],
        },
        move |music_id: String, segment: i32| {
            let mut s = lock(&st);
            s.music_data = s.assets.get_music_data(&s.pack_id, &music_id)?;
            // C++ `musicData.getSegment(segment)` — `std::vector::at`
            // (out-of-range throws); the port surfaces the same failure.
            let time = s
                .music_data
                .get_segment(usize::try_from(segment).map_err(|_| LuaError::ExecutionError {
                    message: "music segment index out of range".into(),
                })?)
                .ok_or_else(|| LuaError::ExecutionError {
                    message: "music segment index out of range".into(),
                })?
                .time;
            s.backend.stop_level_music();
            s.backend.play_level_music_at_time(time);
            Ok(())
        },
    )?
    .arg("musicId")
    .arg("segment")
    .doc("Stop the current music and play the music with id `$0`, starting at segment `$1`. Segments are defined in the music `.json` file, under `\"segments\"`.");

    let st = state.clone();
    add_lua_fn_try!(ctx,
        "a_setMusicSeconds",
        FnMeta {
            ret: "void",
            arg_types: &["std::string", "float"],
        },
        move |music_id: String, time: f32| {
            let mut s = lock(&st);
            s.music_data = s.assets.get_music_data(&s.pack_id, &music_id)?;
            s.backend.stop_level_music();
            s.backend.play_level_music_at_time(time);
            Ok(())
        },
    )?
    .arg("musicId")
    .arg("time")
    .doc("Stop the current music and play the music with id `$0`, starting at time `$1` (in seconds).");

    let st = state.clone();
    add_lua_fn!(ctx,
        "a_playSound",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |sound_id: String| {
            let s = lock(&st);
            s.backend.play_sound_override(&sound_id);
        },
    )?
    .arg("soundId")
    .doc("Play the sound with id `$0`. The id must be registered in `assets.json`, under `\"soundBuffers\"`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "a_playPackSound",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |file_name: String| {
            let pack_id = lock(&st).pack_id.clone();
            let s = lock(&st);
            s.backend.play_pack_sound_override(&pack_id, &file_name);
        },
    )?
    .arg("fileName")
    .doc("Dives into the `Sounds` folder of the current level pack and plays the specified file `$0`.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "a_syncMusicToDM",
        FnMeta {
            ret: "void",
            arg_types: &["bool"],
        },
        move |value: bool| {
            let mut s = lock(&st);
            s.level_status.sync_music_to_dm = value;
            s.backend.refresh_music_pitch();
        },
    )?
    .arg("value")
    .doc("This function, when called, overrides the user's preference of adjusting the music's pitch to the difficulty multiplier. Useful for levels that rely on the music to time events.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "a_setMusicPitch",
        FnMeta {
            ret: "void",
            arg_types: &["float"],
        },
        move |pitch: f32| {
            let mut s = lock(&st);
            s.level_status.music_pitch = pitch;
            s.backend.refresh_music_pitch();
        },
    )?
    .arg("pitch")
    .doc("Manually adjusts the pitch of the music by multiplying it by `$0`. The amount the pitch shifts may change on DM multiplication and user's preference of the music pitch. **Negative values will not work!**");

    let st = state.clone();
    add_lua_fn!(ctx,
        "a_overrideBeepSound",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |file_name: String| {
            let mut s = lock(&st);
            // C++: `levelStatus.beepSound = getPackId() + "_" + fileName`.
            let beep_sound = format!("{}_{}", s.pack_id, file_name);
            s.level_status.beep_sound.clone_from(&beep_sound);
        },
    )?
    .arg("fileName")
    .doc("Dives into the `Sounds` folder of the current level pack and sets the specified file `$0` to be the new beep sound. This only applies to the particular level where this function is called.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "a_overrideIncrementSound",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |file_name: String| {
            let mut s = lock(&st);
            // C++: `levelStatus.levelUpSound = getPackId() + "_" + fileName`.
            let level_up_sound = format!("{}_{}", s.pack_id, file_name);
            s.level_status.level_up_sound.clone_from(&level_up_sound);
        },
    )?
    .arg("fileName")
    .doc("Dives into the `Sounds` folder of the current level pack and sets the specified file `$0` to be the new increment sound. This only applies to the particular level where this function is called.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "a_overrideSwapSound",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |file_name: String| {
            let mut s = lock(&st);
            let swap_sound = format!("{}_{}", s.pack_id, file_name);
            s.level_status.swap_sound.clone_from(&swap_sound);
        },
    )?
    .arg("fileName")
    .doc("Dives into the `Sounds` folder of the current level pack and sets the specified file `$0` to be the new swap sound. This only applies to the particular level where this function is called.");

    let st = state.clone();
    add_lua_fn!(ctx,
        "a_overrideDeathSound",
        FnMeta {
            ret: "void",
            arg_types: &["std::string"],
        },
        move |file_name: String| {
            let mut s = lock(&st);
            let death_sound = format!("{}_{}", s.pack_id, file_name);
            s.level_status.death_sound.clone_from(&death_sound);
        },
    )?
    .arg("fileName")
    .doc("Dives into the `Sounds` folder of the current level pack and sets the specified file `$0` to be the new death sound. This only applies to the particular level where this function is called.");

    Ok(())
}
