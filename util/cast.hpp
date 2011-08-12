/**
 * @file
 *
 * This file contains common casting routines like IntToStr, StrToInt, ...
 *
 * @author Sebastian Meßmer
 */

#pragma once

#ifndef __CAST_HPP__
#define __CAST_HPP__

#include <string>
#include <sstream>
#include <cstdlib>

/**
 * Converts a integer value into a string.
 *
 * @param value The integer value to convert
 * @return The string, representing the integer
 */
const std::string IntToStr(const int value);

/**
 * Converts a string value into an integer.
 * If the string is only in the first part an integer, the rest of the string will be ignored.
 * For example something like "53kdf" will be casted into 53.
 * If the string is completely no integer (e.g. "sfd"), 0 is returned.
 *
 * @param value The string to convert into an integer
 * @return The string parsed as integer
 */
const int StrToInt(const std::string &value);

#include "impl/cast.impl.hpp"

#endif //__CAST_HPP__
