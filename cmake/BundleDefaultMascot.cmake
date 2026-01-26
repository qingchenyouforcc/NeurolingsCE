if(DEFINED OUT)
  get_filename_component(out "${OUT}" ABSOLUTE BASE_DIR "${CMAKE_CURRENT_BINARY_DIR}")
else()
  set(out "${CMAKE_CURRENT_BINARY_DIR}/DefaultMascot.cc")
  if(out STREQUAL "/DefaultMascot.cc")
    set(out "${CMAKE_BINARY_DIR}/DefaultMascot.cc")
  endif()
endif()

if(NOT DEFINED SHIJIMA_DEFAULT_MASCOT_FILES)
  if(NOT DEFINED SHIJIMA_DEFAULT_MASCOT_DIR)
    message(FATAL_ERROR "Either SHIJIMA_DEFAULT_MASCOT_FILES or SHIJIMA_DEFAULT_MASCOT_DIR must be set")
  endif()

  if(NOT DEFINED SHIJIMA_DEFAULT_MASCOT_IMAGE_COUNT)
    set(SHIJIMA_DEFAULT_MASCOT_IMAGE_COUNT 46)
  endif()

  # Defensive: strip accidental quotes from cache/script args.
  string(REPLACE "\"" "" SHIJIMA_DEFAULT_MASCOT_IMAGE_COUNT "${SHIJIMA_DEFAULT_MASCOT_IMAGE_COUNT}")

  set(SHIJIMA_DEFAULT_MASCOT_FILES
    "${SHIJIMA_DEFAULT_MASCOT_DIR}/behaviors.xml"
    "${SHIJIMA_DEFAULT_MASCOT_DIR}/actions.xml"
  )

  foreach(i RANGE 1 ${SHIJIMA_DEFAULT_MASCOT_IMAGE_COUNT})
    list(APPEND SHIJIMA_DEFAULT_MASCOT_FILES "${SHIJIMA_DEFAULT_MASCOT_DIR}/img/shime${i}.png")
  endforeach()
endif()

file(WRITE "${out}" "#include \"shijima-qt/DefaultMascot.hpp\"\n\n")
file(APPEND "${out}" "const std::map<std::string, std::pair<const char *, size_t>> defaultMascot = {\n")

foreach(f IN LISTS SHIJIMA_DEFAULT_MASCOT_FILES)
  get_filename_component(base "${f}" NAME)
  file(READ "${f}" bytes HEX)
  string(LENGTH "${bytes}" hex_len)

  # Convert hex string to \xHH escapes. Keep original length (excluding the terminator).
  math(EXPR byte_len "${hex_len} / 2")
  set(escaped "")

  set(i 0)
  while(i LESS hex_len)
    string(SUBSTRING "${bytes}" ${i} 2 hh)
    string(APPEND escaped "\\x${hh}")
    math(EXPR i "${i} + 2")
  endwhile()

  # Add a null terminator so the data can be treated as a C string.
  string(APPEND escaped "\\x00")

  file(APPEND "${out}" "\t{ \"${base}\", {\n")

  # Split into multiple adjacent string literals to keep line lengths reasonable.
  string(LENGTH "${escaped}" esc_len)
  set(chunk 0)
  while(chunk LESS esc_len)
    math(EXPR take "${esc_len} - ${chunk}")
    if(take GREATER 240)
      set(take 240)
    endif()
    string(SUBSTRING "${escaped}" ${chunk} ${take} part)
    file(APPEND "${out}" "\t\t\"${part}\"\n")
    math(EXPR chunk "${chunk} + ${take}")
  endwhile()

  file(APPEND "${out}" "\t, ${byte_len} } },\n")
endforeach()

file(APPEND "${out}" "};\n")
