// Compiles REAL CWall.cpp / CPlayer.cpp against the REAL SFML fork headers
// (no shims) to emit golden gameplay vectors for the Rust port.
#include "SSVOpenHexagon/Components/CWall.hpp"
#include "SSVOpenHexagon/Components/CPlayer.hpp"
#include "SSVOpenHexagon/Utils/Geometry.hpp"
#include <cstdio>
#include <cstring>
#include <cstdint>

int main(){
    // --- CWall vertex construction (sides=6, side=0, thickness=40, distance=1600) ---
    hg::CWall w(6u, 0.f, 0.f, 0.f, 0.f, {0.f, 0.f}, 0, 40.f, 1600.f,
                hg::SpeedData{5.f}, hg::SpeedData{0.f}, 0.f);
    const auto& v = w.getVertexPositions();
    for(int i=0;i<4;i++) printf("wall_v%d=%.9g,%.9g\n", i, (double)v[i].x, (double)v[i].y);
    printf("wall_overlap_inner_pt=%d\n", (int)w.isOverlapping({1400.f, 0.f}));
    printf("wall_overlap_centroid=%d\n", (int)w.isOverlapping({0.f, 0.f}));

    // --- CWall update: move towards center + curve ---
    hg::CWall w2(6u, 0.f, 0.f, 0.f, 0.f, {0.f,0.f}, 0, 40.f, 1600.f,
                 hg::SpeedData{5.f}, hg::SpeedData{3.f}, 0.f);
    w2.update(1600.f, 75.f, {0.f,0.f}, 1.f);
    const auto& v2 = w2.getVertexPositions();
    for(int i=0;i<4;i++) printf("wall_u_v%d=%.9g,%.9g\n", i, (double)v2[i].x, (double)v2[i].y);
    printf("wall_dead=%d\n", (int)w2.isDead());

    // --- CPlayer triangle draw vertices (sides=6, angle=0.5, size=7.3) ---
    hg::CPlayer p({0.f,0.f}, 8.f, 7.3f, 9.45f, 4.625f);
    p.setPlayerAngle(0.5f);
    p.updatePosition(75.f);
    printf("player_pos=%.9g,%.9g\n", (double)p.getPosition().x, (double)p.getPosition().y);
    printf("player_angle=%.9g\n", (double)p.getPlayerAngle());
    printf("player_ready_to_swap=%d\n", (int)p.isReadyToSwap());
    printf("player_just_swapped=%d\n", (int)p.getJustSwapped());

    // --- player push against a real wall ---
    hg::CWall w3(6u, 0.f, 0.f, 0.f, 0.f, {0.f,0.f}, 0, 40.f, 75.f,
                 hg::SpeedData{0.f}, hg::SpeedData{0.f}, 0.f);
    hg::CPlayer p2({0.f, 0.f}, 8.f, 7.3f, 9.45f, 4.625f);
    p2.setPlayerAngle(0.f); // facing +x
    p2.updatePosition(75.f);
    const float radiusSquared = 75.f*75.f + 8.f;
    bool pushed = p2.push(1, 75.f, w3, {0.f,0.f}, radiusSquared, 1.f);
    printf("push_result=%d\n", (int)pushed);
    printf("push_pos=%.9g,%.9g\n", (double)p2.getPosition().x, (double)p2.getPosition().y);

    // --- line/circle intersection codes ---
    { float x1,y1,x2,y2; float rs=100.f;
      sf::Vector2f a{0.f,0.f}, b{0.f,0.f};
      int code = hg::Utils::getLineCircleIntersection(a, b, {10.f,0.f}, {10.f,10.f}, rs);
      printf("lci_code2=%d a=%.9g,%.9g b=%.9g,%.9g\n", code, (double)a.x,(double)a.y,(double)b.x,(double)b.y); }
    { float rs=1.f; sf::Vector2f a{0.f,0.f}, b{0.f,0.f};
      int code = hg::Utils::getLineCircleIntersection(a, b, {5.f,0.f}, {5.f,10.f}, rs);
      printf("lci_code0=%d\n", code); }
    return 0;
}
