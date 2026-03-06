#pragma once
#include <stdint.h>

// Forward declarations to keep this header C-compatible
#ifdef __cplusplus
extern "C" {
#endif

// Opaque pointer to the C++ Camera object
typedef struct CameraWrapper CameraWrapper;

// Callback signature for Contrast Detection (CD) events
// begin/end are pointers to the array of events
typedef void (*OnCDCallback)(const void *begin, const void *end,
                             void *user_data);

// Constructor / Destructor
CameraWrapper *camera_new_from_first_available();
CameraWrapper *camera_new_from_file(const char *path);
void camera_destroy(CameraWrapper *cam);

// Control
int camera_start(CameraWrapper *cam);
int camera_stop(CameraWrapper *cam);

// Callbacks
// Returns callback ID (>=0) or error code (<0)
int camera_add_cd_callback(CameraWrapper *cam, OnCDCallback cb,
                           void *user_data);

#ifdef __cplusplus
}
#endif
