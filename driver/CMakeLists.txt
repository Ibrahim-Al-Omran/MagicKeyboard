cmake_minimum_required(VERSION 3.20)

project(MagicKeyboardMapping LANGUAGES C)

add_library(mkfilter_mapping STATIC
    src/mapping.c
    src/a1644.c
)

target_include_directories(mkfilter_mapping
    PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}/include
)

set_target_properties(mkfilter_mapping PROPERTIES
    C_STANDARD 11
    C_STANDARD_REQUIRED YES
    C_EXTENSIONS NO
)

if(MSVC)
    target_compile_options(mkfilter_mapping PRIVATE /W4 /WX)
else()
    target_compile_options(mkfilter_mapping PRIVATE
        -Wall
        -Wextra
        -Wpedantic
        -Werror
    )
endif()

include(CTest)
if(BUILD_TESTING)
    add_executable(mkfilter_mapping_tests
        tests/mapping_tests.c
    )
    target_link_libraries(mkfilter_mapping_tests PRIVATE mkfilter_mapping)
    set_target_properties(mkfilter_mapping_tests PROPERTIES
        C_STANDARD 11
        C_STANDARD_REQUIRED YES
        C_EXTENSIONS NO
    )

    if(MSVC)
        target_compile_options(mkfilter_mapping_tests PRIVATE /W4 /WX)
    else()
        target_compile_options(mkfilter_mapping_tests PRIVATE
            -Wall
            -Wextra
            -Wpedantic
            -Werror
        )
    endif()

    add_test(
        NAME mkfilter_mapping_tests
        COMMAND mkfilter_mapping_tests
    )
endif()
