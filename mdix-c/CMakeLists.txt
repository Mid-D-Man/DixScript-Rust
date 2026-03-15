cmake_minimum_required(VERSION 3.16)
project(mdix-c VERSION 1.0.0 LANGUAGES C CXX)

# ── Options ────────────────────────────────────────────────────────────

option(MDIX_BUILD_EXAMPLES "Build usage examples" OFF)
option(MDIX_PREFER_STATIC  "Prefer static library over shared"  OFF)

# ── Locate native library ──────────────────────────────────────────────

set(MDIX_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/include")

if(WIN32)
    set(_MDIX_SEARCH_PATHS
        "${CMAKE_CURRENT_SOURCE_DIR}/lib/windows-x64")
    set(_MDIX_LIB_NAMES "mdix_ffi")
elseif(APPLE)
    set(_MDIX_SEARCH_PATHS
        "${CMAKE_CURRENT_SOURCE_DIR}/lib/macos")
    set(_MDIX_LIB_NAMES "mdix_ffi")
else()
    set(_MDIX_SEARCH_PATHS
        "${CMAKE_CURRENT_SOURCE_DIR}/lib/linux-x64")
    set(_MDIX_LIB_NAMES "mdix_ffi")
endif()

if(MDIX_PREFER_STATIC)
    find_library(MDIX_LIBRARY
        NAMES "libmdix_ffi.a" ${_MDIX_LIB_NAMES}
        PATHS ${_MDIX_SEARCH_PATHS}
        NO_DEFAULT_PATH)
else()
    find_library(MDIX_LIBRARY
        NAMES ${_MDIX_LIB_NAMES}
        PATHS ${_MDIX_SEARCH_PATHS}
        NO_DEFAULT_PATH)
endif()

if(NOT MDIX_LIBRARY)
    message(FATAL_ERROR
        "mdix_ffi library not found in ${_MDIX_SEARCH_PATHS}.\n"
        "Download the pre-built package from GitHub Releases and unzip it\n"
        "so that lib/<platform>/ sits next to this CMakeLists.txt.")
endif()
message(STATUS "[mdix] library: ${MDIX_LIBRARY}")

# ── Imported target ────────────────────────────────────────────────────

add_library(mdix::mdix UNKNOWN IMPORTED GLOBAL)
set_target_properties(mdix::mdix PROPERTIES
    IMPORTED_LOCATION             "${MDIX_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${MDIX_INCLUDE_DIR}")

# On Windows with a shared build, MSVC needs the .lib import library
# as IMPORTED_IMPLIB and the .dll as IMPORTED_LOCATION.
if(WIN32 AND NOT MDIX_PREFER_STATIC)
    find_file(MDIX_DLL
        NAMES "mdix_ffi.dll"
        PATHS ${_MDIX_SEARCH_PATHS}
        NO_DEFAULT_PATH)
    if(MDIX_DLL)
        set_target_properties(mdix::mdix PROPERTIES
            IMPORTED_IMPLIB   "${MDIX_LIBRARY}"
            IMPORTED_LOCATION "${MDIX_DLL}")
        message(STATUS "[mdix] DLL:     ${MDIX_DLL}")
    endif()
endif()

# The C++ wrapper uses C++17 features.
target_compile_features(mdix::mdix INTERFACE cxx_std_17)

# ── Examples ───────────────────────────────────────────────────────────

if(MDIX_BUILD_EXAMPLES)
    add_executable(mdix_hello_c   examples/hello.c)
    add_executable(mdix_hello_cpp examples/hello.cpp)
    target_link_libraries(mdix_hello_c   PRIVATE mdix::mdix)
    target_link_libraries(mdix_hello_cpp PRIVATE mdix::mdix)
    set_target_properties(mdix_hello_cpp PROPERTIES CXX_STANDARD 17)
endif()

# ── Install ────────────────────────────────────────────────────────────

include(GNUInstallDirs)
install(FILES
    include/mdix.h
    include/mdix.hpp
    DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
install(FILES "${MDIX_LIBRARY}"
    DESTINATION ${CMAKE_INSTALL_LIBDIR})
