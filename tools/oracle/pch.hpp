#pragma once
// Force-include the SSVUtils math/frametime helpers that the gameplay sources
// rely on transitively (ssvu::tau/pi, ssvu::FT). Not part of any C++ target;
// only used by the oracle probes to avoid editing the real sources.
#include <SSVUtils/Core/Utils/Math.hpp>
#include <SSVUtils/Core/Common/Frametime.hpp>
