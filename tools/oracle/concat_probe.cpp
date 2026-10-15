// Oracle for Utils::concat float formatting + buildPackId/getLevelValidator,
// since replay filenames embed both and cross-compat depends on exact strings.
#include "SSVOpenHexagon/Utils/Concat.hpp"
#include "SSVOpenHexagon/Utils/BuildPackId.hpp"
#include "SSVOpenHexagon/Utils/LevelValidator.hpp"
#include <cstdio>
#include <string>
int main(){
    printf("concat_float_1.6=%s\n", hg::Utils::concat(1.6f).c_str());
    printf("concat_float_9.45=%s\n", hg::Utils::concat(9.449999809265137f).c_str());
    printf("concat_double_3600=%s\n", hg::Utils::concat(3600.0).c_str());
    printf("concat_int_5=%s\n", hg::Utils::concat(5).c_str());
    printf("concat_mixed=%s\n", hg::Utils::concat("v", '_', std::string("n"), '_', 1.35f, 'x', 60.0).c_str());
    printf("packid=%s\n", hg::Utils::buildPackId("ohvrvanilla","vittorio romeo","base",1).c_str());
    printf("validator=%s\n", hg::Utils::getLevelValidator("ohvrvanilla_vittorio_romeo_cube_1_apeirogon", 1.6f).c_str());
    return 0;
}
