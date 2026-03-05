# Thin wrapper for building ElaWidgetTools as a standalone CMake project.
# Used by the Make build system (MinGW/Linux/macOS) where ElaWidgetTools
# is built in its own build directory rather than via add_subdirectory()
# from the root CMakeLists.txt.
#
# ElaWidgetTools/ElaWidgetTools/CMakeLists.txt does not call find_package(Qt6)
# itself — it expects Qt targets to be inherited from a parent project.
# This wrapper provides them.

cmake_minimum_required(VERSION 3.16)
project(ElaWidgetToolsBuild LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)

# Find Qt6 so that Qt6::Widgets target is available.
find_package(Qt6 REQUIRED COMPONENTS Widgets)

# WidgetsPrivate is needed by ElaWidgetTools for accessing Qt internal APIs.
# On some platforms (e.g., MinGW cross-compile), it's required.
# On others (e.g., Linux with install-qt-action), it may not be available.
find_package(Qt6 COMPONENTS WidgetsPrivate QUIET)
if(NOT Qt6WidgetsPrivate_FOUND)
  message(WARNING "Qt6::WidgetsPrivate not found. ElaWidgetTools may fail to link.")
endif()
# ElaWidgetTools references Qt${QT_VERSION_MAJOR}::Widgets.
if(NOT DEFINED QT_VERSION_MAJOR)
  set(QT_VERSION_MAJOR ${Qt6_VERSION_MAJOR})
endif()

# ELAWIDGETTOOLS_SOURCE_DIR must be passed by the caller (absolute path to
# ElaWidgetTools/ElaWidgetTools/ in the source tree).
if(NOT DEFINED ELAWIDGETTOOLS_SOURCE_DIR)
  message(FATAL_ERROR "ELAWIDGETTOOLS_SOURCE_DIR must be set to the path of ElaWidgetTools/ElaWidgetTools/")
endif()

add_subdirectory("${ELAWIDGETTOOLS_SOURCE_DIR}" ElaWidgetTools)
