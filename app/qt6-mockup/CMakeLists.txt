cmake_minimum_required(VERSION 3.21)
project(NewsQueueQtMockup LANGUAGES CXX)

find_package(Qt6 6.2 REQUIRED COMPONENTS Widgets)
qt_standard_project_setup()
qt_add_executable(newsqueue-qt-compact WIN32 main.cpp)
target_compile_features(newsqueue-qt-compact PRIVATE cxx_std_17)
target_link_libraries(newsqueue-qt-compact PRIVATE Qt6::Widgets)
if(MSVC)
    target_compile_options(newsqueue-qt-compact PRIVATE /utf-8)
endif()

file(GLOB icons CONFIGURE_DEPENDS assets/*.png)
qt_add_resources(newsqueue-qt-compact mockup_icons
    PREFIX "/icons"
    BASE assets
    FILES ${icons} ../assets/newsqueue-16.png ../assets/newsqueue-32.png
)
