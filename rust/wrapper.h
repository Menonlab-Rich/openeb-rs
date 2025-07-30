#pragma once

// --- Core Device and Discovery ---
// These are the most important headers for finding and interacting with a device.
#include "metavision/hal/device/device_discovery.h"
#include "metavision/hal/device/device.h"
#include "metavision/hal/facilities/i_hw_identification.h" // Lets you get info about the hardware.

// --- Core Facilities (Interfaces) ---
// These are the interfaces for the device's capabilities.
// You'll likely need to include the specific ones you want to use.
#include "metavision/hal/facilities/i_events_stream.h" // For getting event data.
#include "metavision/hal/facilities/i_ll_biases.h"     // For controlling sensor biases.
#include "metavision/hal/facilities/i_roi.h"           // For setting a region of interest.
#include "metavision/hal/facilities/i_trigger_in.h"    // For handling input triggers.
#include "metavision/hal/facilities/i_trigger_out.h"   // For controlling output triggers.

// --- Event Decoders ---
// You'll need the decoder for the specific event type your camera produces.
// EventCD is the most common type.
#include "metavision/sdk/base/events/event_cd.h"
