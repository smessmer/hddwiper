#pragma once

#ifndef __ASSEMBLY_HPP__
#define __ASSEMBLY_HPP__

#include "util/thread/ThreadsafeQueue.hpp"
#include "util/thread/Semaphore.hpp"

/**
 * An assembly is a threadsafe queue of products of fixed size.
 * It is used by the producer to store its finished
 * products, until they are fetched.
 *
 * @tparam <Product> The type of products to store
 *
 * @author Sebastian Meßmer
 */
template<class Product>
class Assembly
{
public:
	/**
	 * Create a new assembly.
	 *
	 * @param size The number of elements the assembly can hold.
	 */
	Assembly(const unsigned int size);

	/**
	 * Push an element into the assembly. If there is no space available,
	 * this function blocks, until another thread fetches a product and
	 * therefore frees one space unit.
	 *
	 * @param data The product to push into the assembly
	 */
	void push(const Product &data);

	/**
	 * Fetches an element from the assembly. If there is no finished product
	 * available, this function blocks, until another thread pushes
	 * a product into the assembly.
	 *
	 * @return The finished product
	 */
	const Product pop();

	/**
	 * Returns the number of contained products.
	 *
	 * @return The number of contained products
	 */
	unsigned int size() const;
private:
	//The contained products
	ThreadsafeQueue<Product> _queue;
	//This semaphore blocks the threads that want to push
	//products when the assembly is full.
	//The naming may be a bit confusing - the value of
	//this semaphore is an integer representing the number
	//of empty slots in the assembly.
	Semaphore _empty;
	//This semaphore blocks the threads that want to fetch
	//products when the assembly is empty
	//The naming may be a bit confusing - the value of this
	//semaphore is an integer containing the number of
	//used slots in the assembly. So it's something like
	//the fill grade.
	Semaphore _full;
};

#include "impl/Assembly.impl.hpp"

#endif /* __ASSEMBLY_HPP__ */
