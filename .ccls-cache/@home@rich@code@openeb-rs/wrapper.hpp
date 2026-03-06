// wrapper.hpp

// Base SDK - Events and basic types
#include <metavision/sdk/base/events/event2d.h>
#include <metavision/sdk/base/utils/log.h>

// Stream SDK - Camera control and file reading
#include <metavision/sdk/stream/camera.h>
#include <metavision/sdk/stream/raw_event_file_reader.h>

// Core SDK - Algorithms
#include <metavision/sdk/core/algorithms/events_integration_algorithm.h>
#include <metavision/sdk/core/algorithms/periodic_frame_generation_algorithm.h>

// HAL - Hardware Abstraction Layer (Lower level access)
#include <metavision/hal/device/device.h>
#include <metavision/hal/device/device_discovery.h>
