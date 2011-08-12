#pragma once

#ifndef __KERNELENTROPY_HPP__
#define __KERNELENTROPY_HPP__

#include <boost/function.hpp>

#include "util/data/Data.hpp"
#include "util/DummyCallback.hpp"

/**
 * This is a class with static functions,
 * able to get some kernel entropy (/dev/random).
 *
 * @author Sebastian Meßmer
 */
class KernelEntropy
{
public:
	/**
	 * Get some kernel entropy.
	 *
	 * @param size The number of bytes to receive
	 * @param callback
	 * 		A "progress bar" - callbackfunction that is called
	 * 		after every received byte,
	 * 		until there are enough bytes to return them.
	 * 		The callback function has one parameter, which is the number of already read bytes.
	 * 		This parameter is optional.
	 */
	static const Data getEntropy(const unsigned int size, boost::function<void(
			unsigned int)> callback = DummyCallback());
};

#include "impl/KernelEntropy.impl.hpp"

#endif
