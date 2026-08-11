cmake_minimum_required(VERSION 3.20)
project(dispatch_replan_solver LANGUAGES CXX)

if(NOT ORTOOLS_SOURCE_DIR)
  message(FATAL_ERROR "ORTOOLS_SOURCE_DIR is required")
endif()

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(BUILD_DEPS ON CACHE BOOL "" FORCE)
set(BUILD_TESTING OFF CACHE BOOL "" FORCE)
set(BUILD_SAMPLES OFF CACHE BOOL "" FORCE)
set(BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(BUILD_CXX_SAMPLES OFF CACHE BOOL "" FORCE)
set(BUILD_CXX_EXAMPLES OFF CACHE BOOL "" FORCE)
set(BUILD_FLATZINC OFF CACHE BOOL "" FORCE)
set(BUILD_MATH_OPT OFF CACHE BOOL "" FORCE)
set(BUILD_PYTHON OFF CACHE BOOL "" FORCE)
set(BUILD_JAVA OFF CACHE BOOL "" FORCE)
set(BUILD_DOTNET OFF CACHE BOOL "" FORCE)
set(USE_COINOR OFF CACHE BOOL "" FORCE)
set(USE_GLPK OFF CACHE BOOL "" FORCE)
set(USE_HIGHS OFF CACHE BOOL "" FORCE)
set(USE_SCIP OFF CACHE BOOL "" FORCE)
set(USE_PDLP OFF CACHE BOOL "" FORCE)
set(USE_BOP OFF CACHE BOOL "" FORCE)
set(USE_CPLEX OFF CACHE BOOL "" FORCE)
set(USE_GUROBI OFF CACHE BOOL "" FORCE)
set(USE_XPRESS OFF CACHE BOOL "" FORCE)

add_subdirectory(${ORTOOLS_SOURCE_DIR} ${CMAKE_BINARY_DIR}/or-tools)

add_executable(dispatch_replan_solver bridge/dispatch_replan_solver.cc)
target_compile_features(dispatch_replan_solver PRIVATE cxx_std_17)
target_link_libraries(dispatch_replan_solver PRIVATE ortools::ortools)
target_link_options(dispatch_replan_solver PRIVATE
  --bind
  -O3
  -sASSERTIONS=0
  -sALLOW_MEMORY_GROWTH=1
  -sENVIRONMENT=web,worker,node
  -sEXPORT_ES6=1
  -sFILESYSTEM=0
  -sINITIAL_MEMORY=67108864
  -sMODULARIZE=1
  -sSTACK_SIZE=1048576
)
set_target_properties(dispatch_replan_solver PROPERTIES SUFFIX ".js")
