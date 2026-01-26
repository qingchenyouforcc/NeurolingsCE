cmake_minimum_required(VERSION 3.21)

project(ShijimaQt LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)

set(CMAKE_PREFIX_PATH "D:/Qt/6.8.3/msvc2022_64")

option(SHIJIMA_USE_QTMULTIMEDIA "Enable Qt Multimedia support" ON)

find_package(Qt6 REQUIRED COMPONENTS Core Gui Widgets Concurrent)
message(STATUS "Using Qt6 from: ${Qt6_DIR}")
message(STATUS "Qt6 version: ${Qt6_VERSION}")

function(_shijima_print_qt_target_locations tgt)
  if(NOT TARGET ${tgt})
    return()
  endif()

  foreach(cfg DEBUG RELEASE RELWITHDEBINFO MINSIZEREL)
    get_target_property(_loc ${tgt} IMPORTED_LOCATION_${cfg})
    if(_loc)
      message(STATUS "${tgt} ${cfg}: ${_loc}")
    endif()
  endforeach()
endfunction()

_shijima_print_qt_target_locations(Qt6::Core)
_shijima_print_qt_target_locations(Qt6::Concurrent)

# Conda-provided Qt packages are typically release-only on Windows (no *d.lib).
# In that case, Debug builds must use the release CRT (/MD) to avoid CRT conflicts.
set(_shijima_msvc_runtime "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL")
if(MSVC)
  string(TOLOWER "${Qt6_DIR}" _qt6_dir_lower)
  if(_qt6_dir_lower MATCHES "miniconda|anaconda|conda")
    set(_shijima_msvc_runtime "MultiThreadedDLL")
    message(WARNING "Detected conda Qt6; forcing /MD runtime for all configs to avoid Debug CRT mismatches.")
  endif()
endif()
if(SHIJIMA_USE_QTMULTIMEDIA)
  find_package(Qt6 COMPONENTS Multimedia QUIET)
  if(NOT TARGET Qt6::Multimedia)
    message(WARNING "Qt6 Multimedia not found; building without sound support (SHIJIMA_USE_QTMULTIMEDIA=OFF).")
    set(SHIJIMA_USE_QTMULTIMEDIA OFF CACHE BOOL "Enable Qt Multimedia support" FORCE)
  endif()
endif()

# Dependencies
add_subdirectory(libshijima)

if(MSVC AND TARGET shijima)
  set_property(TARGET shijima PROPERTY MSVC_RUNTIME_LIBRARY "${_shijima_msvc_runtime}")
endif()

# libshimejifinder's upstream CMake uses bash scripts and Unix headers in some files.
# For MSVC builds we build it as an external dependency later, or you can use a
# prebuilt libshimejifinder and point CMake to it.
option(SHIJIMA_WITH_LIBSHIMEJIFINDER "Build with bundled libshimejifinder" OFF)

if(SHIJIMA_WITH_LIBSHIMEJIFINDER)
  add_subdirectory(libshimejifinder)
endif()

option(SHIJIMA_WITH_DEFAULT_MASCOT "Embed DefaultMascot assets" ON)
option(SHIJIMA_WITH_LICENSES_TEXT "Embed licenses text" ON)

set(_shijima_generated_dir "${CMAKE_CURRENT_BINARY_DIR}")
set(_shijima_licenses_header "${_shijima_generated_dir}/licenses_generated.hpp")

set(_SHIJIMA_WITH_SHIMEJIFINDER_DEFAULT ON)
if(MSVC)
  # libshimejifinder depends on nested submodules and Unix tooling; keep it off by default on MSVC.
  set(_SHIJIMA_WITH_SHIMEJIFINDER_DEFAULT OFF)
endif()
option(SHIJIMA_WITH_SHIMEJIFINDER "Enable archive import support" ${_SHIJIMA_WITH_SHIMEJIFINDER_DEFAULT})

add_executable(shijima-qt)

if(MSVC)
  set_property(TARGET shijima-qt PROPERTY MSVC_RUNTIME_LIBRARY "${_shijima_msvc_runtime}")
endif()

target_sources(shijima-qt PRIVATE
  src/app/main.cc
  src/app/Asset.cc
  src/app/MascotData.cc
  src/app/AssetLoader.cc
  src/app/ForcedProgressDialog.cc
  src/app/ShijimaContextMenu.cc
  src/app/ShijimaManager.cc
  src/app/ShijimaWidget.cc
  src/app/SoundEffectManager.cc
  src/app/ShijimaLicensesDialog.cc
  src/app/ShimejiInspectorDialog.cc
  src/app/ShijimaHttpApi.cc
  src/app/cli.cc

  src/platform/Platform/Platform.hpp
  src/platform/Platform/ActiveWindow.hpp
  src/platform/Platform/ActiveWindowObserver.hpp
)

if(SHIJIMA_WITH_DEFAULT_MASCOT)
  set(_default_mascot_files
    "${CMAKE_CURRENT_SOURCE_DIR}/src/assets/DefaultMascot/behaviors.xml"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/assets/DefaultMascot/actions.xml"
  )
  foreach(i RANGE 1 46)
    list(APPEND _default_mascot_files "${CMAKE_CURRENT_SOURCE_DIR}/src/assets/DefaultMascot/img/shime${i}.png")
  endforeach()

  add_custom_command(
    OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/DefaultMascot.cc"
    COMMAND ${CMAKE_COMMAND}
      -DOUT:FILEPATH=${CMAKE_CURRENT_BINARY_DIR}/DefaultMascot.cc
      -DSHIJIMA_DEFAULT_MASCOT_DIR:PATH=${CMAKE_CURRENT_SOURCE_DIR}/src/assets/DefaultMascot
      -DSHIJIMA_DEFAULT_MASCOT_IMAGE_COUNT:STRING=46
      -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/BundleDefaultMascot.cmake"
    DEPENDS
      "${CMAKE_CURRENT_SOURCE_DIR}/cmake/BundleDefaultMascot.cmake"
      ${_default_mascot_files}
    VERBATIM
  )

  add_custom_target(shijima_default_mascot_generated DEPENDS "${CMAKE_CURRENT_BINARY_DIR}/DefaultMascot.cc")
  add_dependencies(shijima-qt shijima_default_mascot_generated)

  set_source_files_properties("${CMAKE_CURRENT_BINARY_DIR}/DefaultMascot.cc" PROPERTIES GENERATED TRUE)
  target_sources(shijima-qt PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/DefaultMascot.cc")
  target_include_directories(shijima-qt PRIVATE ${CMAKE_CURRENT_BINARY_DIR})
else()
  message(FATAL_ERROR "SHIJIMA_WITH_DEFAULT_MASCOT=OFF is not supported yet (DefaultMascot.cc is required).")
endif()

if(SHIJIMA_WITH_LICENSES_TEXT)
  add_custom_command(
    OUTPUT "${_shijima_licenses_header}"
    COMMAND ${CMAKE_COMMAND}
      -DOUT:FILEPATH=${_shijima_licenses_header}
      -DSHIJIMA_SOURCE_DIR:PATH=${CMAKE_CURRENT_SOURCE_DIR}
      -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/GenerateLicenses.cmake"
    DEPENDS
      ${CMAKE_CURRENT_SOURCE_DIR}/cmake/GenerateLicenses.cmake
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/Shijima-Qt.LICENSE.txt
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/duktape.LICENSE.txt
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/duktape.AUTHORS.rst
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/libarchive.LICENSE.txt
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/libshijima.LICENSE.txt
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/libshimejifinder.LICENSE.txt
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/unarr.LICENSE.txt
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/unarr.AUTHORS.txt
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/Qt.LICENSE.txt
      ${CMAKE_CURRENT_SOURCE_DIR}/licenses/rapidxml.LICENSE.txt
    VERBATIM
  )

  add_custom_target(shijima_licenses_generated DEPENDS "${_shijima_licenses_header}")
  add_dependencies(shijima-qt shijima_licenses_generated)
  target_include_directories(shijima-qt PRIVATE ${CMAKE_CURRENT_BINARY_DIR})
else()
  message(FATAL_ERROR "SHIJIMA_WITH_LICENSES_TEXT=OFF is not supported yet (licenses_generated.hpp is required).")
endif()

# Ensure the generated header exists before compiling the file that includes it.
set_source_files_properties(src/app/ShijimaLicensesDialog.cc PROPERTIES OBJECT_DEPENDS "${_shijima_licenses_header}")

if(WIN32)
  target_sources(shijima-qt PRIVATE
    src/resources/resources.rc
    src/platform/Platform/Windows/Platform.cc
    src/platform/Platform/Windows/ActiveWindowObserver.cc
    src/platform/Platform/Windows/PrivateActiveWindowObserver.cc
    src/platform/Platform/Windows/PrivateActiveWindowObserver.hpp
  )
elseif(APPLE)
  target_sources(shijima-qt PRIVATE
    src/platform/Platform/macOS/Platform.mm
    src/platform/Platform/macOS/ActiveWindowObserver.cc
    src/platform/Platform/macOS/PrivateActiveWindowObserver.mm
    src/platform/Platform/macOS/PrivateActiveWindowObserver.hpp
  )
elseif(UNIX)
  target_sources(shijima-qt PRIVATE
    src/platform/Platform/Linux/Platform.cc
    src/platform/Platform/Linux/ActiveWindowObserver.cc
    src/platform/Platform/Linux/PrivateActiveWindowObserver.cc
    src/platform/Platform/Linux/PrivateActiveWindowObserver.hpp
    src/platform/Platform/Linux/KWin.cc
    src/platform/Platform/Linux/KDEWindowObserverBackend.cc
    src/platform/Platform/Linux/GNOME.cc
    src/platform/Platform/Linux/GNOMEWindowObserverBackend.cc
    src/platform/Platform/Linux/DBus.cc
    src/platform/Platform/Linux/ExtensionFile.cc
  )
endif()

target_include_directories(shijima-qt PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}
  ${CMAKE_CURRENT_SOURCE_DIR}/include
  ${CMAKE_CURRENT_SOURCE_DIR}/src/platform
  ${CMAKE_CURRENT_SOURCE_DIR}/libshijima
  ${CMAKE_CURRENT_SOURCE_DIR}/libshimejifinder
  ${CMAKE_CURRENT_SOURCE_DIR}/cpp-httplib
)

target_compile_definitions(shijima-qt PRIVATE
  $<$<BOOL:${SHIJIMA_USE_QTMULTIMEDIA}>:SHIJIMA_USE_QTMULTIMEDIA=1>
  $<$<NOT:$<BOOL:${SHIJIMA_USE_QTMULTIMEDIA}>>:SHIJIMA_USE_QTMULTIMEDIA=0>
  $<$<BOOL:${SHIJIMA_WITH_SHIMEJIFINDER}>:SHIJIMA_WITH_SHIMEJIFINDER=1>
  $<$<NOT:$<BOOL:${SHIJIMA_WITH_SHIMEJIFINDER}>>:SHIJIMA_WITH_SHIMEJIFINDER=0>
)

target_link_libraries(shijima-qt PRIVATE
  Qt6::Core
  Qt6::Gui
  Qt6::Widgets
  Qt6::Concurrent
  shijima
)

if(SHIJIMA_WITH_SHIMEJIFINDER)
  if(SHIJIMA_WITH_LIBSHIMEJIFINDER)
    target_link_libraries(shijima-qt PRIVATE shimejifinder)
    target_compile_definitions(shijima-qt PRIVATE SHIMEJIFINDER_NO_LIBARCHIVE=0 SHIMEJIFINDER_NO_LIBUNARR=0)
  else()
    message(FATAL_ERROR "SHIJIMA_WITH_SHIMEJIFINDER=ON requires SHIJIMA_WITH_LIBSHIMEJIFINDER=ON for now.")
  endif()
else()
  target_compile_definitions(shijima-qt PRIVATE
    SHIMEJIFINDER_NO_LIBARCHIVE=1
    SHIMEJIFINDER_NO_LIBUNARR=1
  )
endif()

if(SHIJIMA_USE_QTMULTIMEDIA)
  target_link_libraries(shijima-qt PRIVATE Qt6::Multimedia)
endif()

if(WIN32)
  target_compile_definitions(shijima-qt PRIVATE NOMINMAX WIN32_LEAN_AND_MEAN)
  set_target_properties(shijima-qt PROPERTIES WIN32_EXECUTABLE TRUE)
endif()
