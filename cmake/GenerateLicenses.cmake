if(DEFINED OUT)
  get_filename_component(out "${OUT}" ABSOLUTE BASE_DIR "${CMAKE_CURRENT_BINARY_DIR}")
else()
  set(out "${CMAKE_CURRENT_BINARY_DIR}/licenses_generated.hpp")
endif()

if(NOT DEFINED SHIJIMA_SOURCE_DIR)
  get_filename_component(SHIJIMA_SOURCE_DIR "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
endif()

file(WRITE "${out}" "#pragma once\n\n")
file(APPEND "${out}" "static const char *shijima_licenses =\n")

set(_DELIM "SHIJIMA_LICENSES")

function(_append_raw outfile text)
  # MSVC has a fairly small max string-literal size; chunk aggressively.
  set(_chunk_size 16000)
  string(LENGTH "${text}" _len)
  set(_pos 0)
  while(_pos LESS _len)
    math(EXPR _take "${_len} - ${_pos}")
    if(_take GREATER ${_chunk_size})
      set(_take ${_chunk_size})
    endif()
    string(SUBSTRING "${text}" ${_pos} ${_take} _part)

    file(APPEND "${outfile}" "R\"${_DELIM}(")
    file(APPEND "${outfile}" "${_part}")
    file(APPEND "${outfile}" ")${_DELIM}\"\n")

    math(EXPR _pos "${_pos} + ${_take}")
  endwhile()
endfunction()

_append_raw("${out}" "\nLicenses for the software components used in Shijima-Qt are listed below.\n")

set(_license_files
  "${SHIJIMA_SOURCE_DIR}/licenses/Shijima-Qt.LICENSE.txt"
  "${SHIJIMA_SOURCE_DIR}/licenses/duktape.LICENSE.txt"
  "${SHIJIMA_SOURCE_DIR}/licenses/duktape.AUTHORS.rst"
  "${SHIJIMA_SOURCE_DIR}/licenses/libarchive.LICENSE.txt"
  "${SHIJIMA_SOURCE_DIR}/licenses/libshijima.LICENSE.txt"
  "${SHIJIMA_SOURCE_DIR}/licenses/libshimejifinder.LICENSE.txt"
  "${SHIJIMA_SOURCE_DIR}/licenses/unarr.LICENSE.txt"
  "${SHIJIMA_SOURCE_DIR}/licenses/unarr.AUTHORS.txt"
  "${SHIJIMA_SOURCE_DIR}/licenses/Qt.LICENSE.txt"
  "${SHIJIMA_SOURCE_DIR}/licenses/rapidxml.LICENSE.txt"
)

foreach(f IN LISTS _license_files)
  get_filename_component(base "${f}" NAME)
  file(READ "${f}" contents)
  _append_raw("${out}" "\n\n~~~~~~~~~~ ${base} ~~~~~~~~~~\n\n")
  _append_raw("${out}" "${contents}")
endforeach()

file(APPEND "${out}" ";\n")
