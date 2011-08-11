#pragma once

#ifndef __PROFILER_HPP__
#define __PROFILER_HPP__

#include <sys/time.h>

/**
 * An instance of this class can measure time intervals.
 * This is extremely useful for profiling applications.
 *
 * @author Sebastian Meßmer
 */
class Profiler
{
public:
	/**
	 * Create a new profiler and start the clock
	 */
	Profiler();

	/**
	 * Return the number of seconds since the creation
	 * of the profiler object.
	 * This return value is double and has a resolution of microseconds
	 *
	 * @return The number of seconds since the creation of the profiler object.
	 */
	double getSecondsPassed() const;

private:
	unsigned long long int currentTime() const;

	unsigned long long int _start;
};

#include "impl/Profiler.impl.hpp"

#endif
