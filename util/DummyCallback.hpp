#pragma once

#ifndef __DUMMYCALLBACK_HPP__
#define __DUMMYCALLBACK_HPP__

/**
 * This class is a dummy callback functor
 * (for a function taking one unsigned int parameter)
 * that does exactly nothing when it's called.
 *
 * @author Sebastian Meßmer
 */
class DummyCallback
{
public:
	/**
	 * Call the callback and ... do exactly nothing
	 */
	void operator()(unsigned int);
};

#include "impl/DummyCallback.impl.hpp"

#endif
