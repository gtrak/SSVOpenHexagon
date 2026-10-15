//! Vertex-ordering / draw-batch tests and CCustomWall + CCustomWallManager
//! handle-reuse + flag tests (task spec: vertex-ordering test; §4 manager
//! handle allocation/reuse + flags).

use oh_game::{
    vertex_batch::FastVertexVectorTris, CCustomWall, CCustomWallManager, CPlayer, CWall, SpeedData,
};
use oh_math::hg::color::Color;
use oh_math::ssvs::Vec2;

// --- Vertex ordering: CWall::draw emits (nw, sw, se, nw, se, ne) ----------

#[test]
fn cwall_draw_quad_split_order() {
    let center = Vec2::new(0.0, 0.0);
    let speed = SpeedData::new(0.0, 0.0, 0.0, 0.0, false);
    let curve = SpeedData::new(0.0, 0.0, 0.0, 0.0, false);
    let w = CWall::new(
        6, 0.0, 0.0, 0.0, 0.0, &center, 0, 40.0, 1600.0, &speed, &curve, 0.0,
    );
    let v = w.get_vertex_positions();

    let mut batch = FastVertexVectorTris::new();
    batch.reserve_more_quad(1);
    w.draw(Color::WHITE, &mut batch);

    let got = batch.as_slice();
    assert_eq!(got.len(), 6, "a quad is two triangles (6 vertices)");
    // Triangles (nw, sw, se) and (nw, se, ne) on the (0,2) diagonal.
    let want = [v[0], v[1], v[2], v[0], v[2], v[3]];
    for (i, (g, wpos)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!((g.position.x, g.position.y), (wpos.x, wpos.y), "vertex {i}");
    }
}

// --- CCustomWall flags + old-vertex capture -------------------------------

#[test]
fn custom_wall_flags_and_old_vertex_capture() {
    let mut cw = CCustomWall::new();
    assert!(cw.get_can_collide(), "default: collides");
    assert!(!cw.get_deadly(), "default: not deadly");
    assert_eq!(cw.get_killing_side(), 0);
    assert!(cw.is_custom_wall());

    cw.set_can_collide(false);
    assert!(!cw.get_can_collide());
    cw.set_can_collide(true);
    assert!(cw.get_can_collide());
    cw.set_deadly(true);
    assert!(cw.get_deadly());

    // setVertexPos swaps the old value into _oldVertexPositions.
    let a = Vec2::new(1.0, 2.0);
    cw.set_vertex_pos(0, a);
    assert_eq!(cw.get_vertex_pos(0), a);
    assert_eq!(cw.get_old_vertex_positions()[0], Vec2::new(0.0, 0.0));
    let b = Vec2::new(3.0, 4.0);
    cw.set_vertex_pos(0, b);
    assert_eq!(cw.get_vertex_pos(0), b);
    assert_eq!(
        cw.get_old_vertex_positions()[0],
        a,
        "old vertex now the previous value"
    );
}

#[test]
fn custom_wall_draw_per_vertex_color_quad() {
    let mut cw = CCustomWall::new();
    cw.set_vertex_pos(0, Vec2::new(0.0, 0.0));
    cw.set_vertex_pos(1, Vec2::new(0.0, 10.0));
    cw.set_vertex_pos(2, Vec2::new(10.0, 10.0));
    cw.set_vertex_pos(3, Vec2::new(10.0, 0.0));
    cw.set_vertex_color(0, Color::RED);
    cw.set_vertex_color(1, Color::GREEN);
    cw.set_vertex_color(2, Color::BLUE);
    cw.set_vertex_color(3, Color::WHITE);

    let mut batch = FastVertexVectorTris::new();
    cw.draw(&mut batch);
    let got = batch.as_slice();
    assert_eq!(got.len(), 6);
    // (nw, sw, se, nw, se, ne) with matching per-vertex colors.
    let want_pos = [
        (0.0, 0.0),
        (0.0, 10.0),
        (10.0, 10.0),
        (0.0, 0.0),
        (10.0, 10.0),
        (10.0, 0.0),
    ];
    let want_col = [
        Color::RED,
        Color::GREEN,
        Color::BLUE,
        Color::RED,
        Color::BLUE,
        Color::WHITE,
    ];
    for i in 0..6 {
        assert_eq!(
            (got[i].position.x, got[i].position.y),
            want_pos[i],
            "pos {i}"
        );
        assert_eq!(got[i].color, want_col[i], "color {i}");
    }
}

// --- CCustomWallManager handle allocation / reuse / flags ----------------

#[test]
fn custom_wall_manager_handle_reuse() {
    let mut mgr = CCustomWallManager::new();
    let h1 = mgr.create(|cw| {
        cw.set_vertex_pos(0, Vec2::new(0.0, 0.0));
        cw.set_vertex_pos(1, Vec2::new(0.0, 1.0));
        cw.set_vertex_pos(2, Vec2::new(1.0, 1.0));
        cw.set_vertex_pos(3, Vec2::new(1.0, 0.0));
    });
    assert_eq!(mgr.count(), 1);
    assert_eq!(
        mgr.max_handles(),
        32,
        "first block reserves 32 + 2*0 handles"
    );

    // `back()`/`pop_back()` => the first live handle is the highest reserved
    // index (31), matching the C++ free-list order.
    assert_eq!(h1, 31);

    mgr.destroy(h1);
    assert_eq!(mgr.count(), 0);

    // Re-creation reuses the freed handle (it sits at the free-list back).
    let h2 = mgr.create(|_| {});
    assert_eq!(h2, h1, "handle reused after destroy");
    assert_eq!(mgr.count(), 1);

    // A destroyed handle is an "available" handle: getters return sentinels.
    mgr.destroy(h2);
    assert_eq!(
        mgr.get_killing_side(h2),
        255,
        "invalid handle -> -1 as u8 (255)"
    );
    assert!(!mgr.get_can_collide(h2));
    assert!(!mgr.get_deadly(h2));
    assert_eq!(mgr.get_vertex_pos(h2, 0), Vec2::new(0.0, 0.0));
}

#[test]
fn custom_wall_manager_setters_and_getters() {
    let mut mgr = CCustomWallManager::new();
    let h = mgr.create(|cw| {
        cw.set_deadly(true);
        cw.set_killing_side(2);
    });

    assert!(mgr.get_deadly(h));
    assert_eq!(mgr.get_killing_side(h), 2);
    assert!(mgr.get_can_collide(h));
    mgr.set_can_collide(h, false);
    assert!(!mgr.get_can_collide(h));

    // Invalid vertex index is a no-op.
    mgr.set_vertex_pos(h, 5, Vec2::new(9.0, 9.0));
    assert_eq!(mgr.get_vertex_pos(h, 0), Vec2::new(0.0, 0.0));
}

// --- Custom-wall push: justSwapped short-circuit via the manager ----------

#[test]
fn custom_wall_manager_kill_on_deadly_overlap() {
    let mut mgr = CCustomWallManager::new();
    // A deadly wall covering the player's position.
    let _ = mgr.create(|cw| {
        cw.set_deadly(true);
        cw.set_vertex_pos(0, Vec2::new(-10.0, -10.0));
        cw.set_vertex_pos(1, Vec2::new(-10.0, 10.0));
        cw.set_vertex_pos(2, Vec2::new(10.0, 10.0));
        cw.set_vertex_pos(3, Vec2::new(10.0, -10.0));
    });

    let origin = Vec2::new(0.0, 0.0);
    let mut p = CPlayer::new(&origin, 8.0, 7.3, 9.45, 4.625);
    p.set_player_angle(0.0);
    p.update_position(75.0);
    // Move the player into the (small) deadly wall: radius 75 puts the player
    // at (75, 0), which is OUTSIDE this wall. Re-project at a tiny radius so the
    // player sits at (0, 0), inside the deadly wall.
    p.update_position(0.0);

    let killed = mgr.handle_collision(1, 0.0, &mut p, 1.0);
    assert!(killed, "deadly overlap (and player moving) should kill");
}
