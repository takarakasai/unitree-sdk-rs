/* bindgen entry point for the Cyclone DDS C API.
 *
 * We pull in the umbrella header; the build script's allowlist restricts the
 * generated bindings to the `dds_*` / `DDS_*` surface we actually use. */
#include <dds/dds.h>
