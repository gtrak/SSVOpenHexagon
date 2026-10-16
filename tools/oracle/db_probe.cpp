// C++ oracle probe: the exact `ohdb.sqlite` schema the REAL
// `src/SSVOpenHexagon/Online/Database.cpp` creates.
//
// `Database.cpp` builds its sqlite_orm storage against the hardcoded
// relative path `"ohdb.sqlite"` (`Database.cpp:48`) and calls
// `sync_schema(true /* preserve */)` (`Database.cpp:71`); this probe
// compiles the real `Database.cpp` (vendored `public/sqlite_orm/sqlite_orm.h`
// + the system SQLite C API), triggers the static storage, and dumps
// `sqlite_master` — the `.schema` output a `sqlite3 ohdb.sqlite` session
// would show. The dump is the reference the Rust schema
// (`crates/oh-net/src/database.rs`) must reproduce byte-for-byte
// (`crates/oh-net/tests/database.rs` asserts it).
//
// Build + run (from the repo root; the extlibs submodules must be checked
// out at $ROOT — the probe needs the vrm_pp + SSVUtils headers; the
// worktrees keep them in the main checkout):
//
//   EXTLIBS=/home/gary/dev/SSVOpenHexagon   # main checkout (submodules)
//   g++ -std=c++20 -O0 -include cstdint \
//     -Iinclude -I. \
//     -Ipublic/sqlite -Ipublic/sqlite_orm \
//     -I$EXTLIBS/extlibs/vrm_pp/include -I$EXTLIBS/extlibs/SSVUtils/include \
//     -o /tmp/ohdb_probe tools/oracle/db_probe.cpp tools/oracle/stubs.cpp \
//     src/SSVOpenHexagon/Online/Database.cpp \
//     src/SSVOpenHexagon/Utils/Timestamp.cpp \
//     -lsqlite3
//
//   /tmp/ohdb_probe /tmp/ohdb_probe_dir
//
// Output (deterministic — fresh directory every run):
//
//   === ohdb.sqlite schema (C++ sqlite_orm, real Database.cpp) ===
//   CREATE TABLE "users" ( ... )
//   CREATE TABLE "loginTokens" ( ... )
//   CREATE TABLE "scores" ( ... )
//   === ok ===

#include "SSVOpenHexagon/Online/Database.hpp"

#include <sqlite3.h>

#include <cstdio>
#include <cstdlib>
#include <string>

#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

int main(const int argc, char** argv)
{
    const std::string dir =
        (argc > 1) ? std::string(argv[1]) : std::string("/tmp/ohdb_probe_dir");

    if(mkdir(dir.c_str(), 0755) != 0 && access(dir.c_str(), F_OK) != 0)
    {
        std::fprintf(stderr, "cannot create directory '%s'\n", dir.c_str());
        return 1;
    }

    if(chdir(dir.c_str()) != 0)
    {
        std::fprintf(stderr, "cannot chdir to '%s'\n", dir.c_str());
        return 1;
    }

    // Fresh DB: the C++ storage hardcodes the relative name "ohdb.sqlite"
    // (Database.cpp:48), so the schema is always rebuilt from scratch.
    std::remove("ohdb.sqlite");

    // Trigger the static storage: getStorage() -> makeStorage() ->
    // sync_schema(true) creates the three tables (Database.cpp:44-76).
    // dumpUsers() is the cheapest touch (a get_all over an empty table).
    hg::Database::dumpUsers();

    sqlite3* db = nullptr;
    if(sqlite3_open("ohdb.sqlite", &db) != SQLITE_OK)
    {
        std::fprintf(stderr, "sqlite3_open failed: %s\n", sqlite3_errmsg(db));
        return 2;
    }

    std::printf("=== ohdb.sqlite schema (C++ sqlite_orm, real "
                "Database.cpp) ===\n");

    // The sqlite3 CLI's `.schema` command prints each object's stored
    // CREATE statement (sqlite_master.sql) in rowid (creation) order;
    // that is exactly this dump.
    const char* dumpSql =
        "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY rowid";

    sqlite3_stmt* stmt = nullptr;
    if(sqlite3_prepare_v2(db, dumpSql, -1, &stmt, nullptr) != SQLITE_OK)
    {
        std::fprintf(stderr, "prepare failed: %s\n", sqlite3_errmsg(db));
        sqlite3_close(db);
        return 3;
    }

    int failures = 0;
    while(sqlite3_step(stmt) == SQLITE_ROW)
    {
        const char* sqlText =
            reinterpret_cast<const char*>(sqlite3_column_text(stmt, 0));

        std::printf("%s\n", (sqlText != nullptr) ? sqlText : "(null)");
        if(sqlText == nullptr)
        {
            ++failures;
        }
    }

    sqlite3_finalize(stmt);
    sqlite3_close(db);

    std::printf("=== %s ===\n", (failures == 0) ? "ok" : "FAIL");
    return (failures == 0) ? 0 : 4;
}
