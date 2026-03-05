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
  message(WARNING "Qt6::WidgetsPrivate not found. Providing private header include paths manually.")
  # Create a stand-in INTERFACE target so ElaWidgetTools' target_link_libraries
  # against Qt6::WidgetsPrivate does not fail.
  if(NOT TARGET Qt6::WidgetsPrivate)
    add_library(Qt6::WidgetsPrivate INTERFACE IMPORTED)
    # Qt private headers include each other using <QtModule/private/header_p.h>,
    # so we need the Qt include root on the search path.
    get_target_property(_qt_widgets_inc Qt6::Widgets INTERFACE_INCLUDE_DIRECTORIES)
    foreach(_dir IN LISTS _qt_widgets_inc)
      # The Qt include root is the parent of the module-specific include dir.
      # e.g. /path/to/Qt/6.8.2/gcc_64/include/QtWidgets -> /path/to/Qt/6.8.2/gcc_64/include
      get_filename_component(_parent "${_dir}" DIRECTORY)
      if(IS_DIRECTORY "${_parent}/QtWidgets/private")
        set(_qt_inc_root "${_parent}")
        break()
      endif()
    endforeach()
    if(DEFINED _qt_inc_root)
      message(STATUS "Using private headers from: ${_qt_inc_root}")
      set_target_properties(Qt6::WidgetsPrivate PROPERTIES
        INTERFACE_INCLUDE_DIRECTORIES "${_qt_inc_root};${_qt_inc_root}/QtWidgets;${_qt_inc_root}/QtGui;${_qt_inc_root}/QtCore"
      )
    else()
      message(WARNING "Could not locate Qt private header directories.")
    endif()
  endif()
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
