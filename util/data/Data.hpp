#pragma once

#ifndef __DATA_HPP__
#define __DATA_HPP__

#include <tr1/memory>
#include <stdexcept>

#include "DataBase.hpp"

/**
 * An instance of this class represents a specific amount of data.
 * The constructor allocates it and the destructor frees it.
 * When the Data object gets copied, the original and the copy
 * share the same data area. So changes made to one will affect the other!
 *
 * @author Sebastian Meßmer
 */
class Data
{
public:
	/**
	 * Allocate a new data area and create the resource handler object (of class Data)
	 *
	 * @param size The size in bytes of the area to allocate
	 */
	Data(const unsigned int size);

	/**
	 * Return a pointer to the first byte of the data area.
	 *
	 * @return A pointer to the first byte of the data area
	 */
	unsigned char *get();

	/**
	 * Return a pointer to the first byte of the data area.
	 *
	 * @return A pointer to the first byte of the data area
	 */
	const unsigned char *get() const;

	/**
	 * Return the size of the allocated data area
	 *
	 * @return The size of the allocated data area
	 */
	unsigned int size() const;
	
	/**
	 * Return a subsegment of the stored data. The segment is copied.
	 *
	 * @param start The start offset
	 * @param length The number of bytes to return from the start offset on
	 * @return A copy of this sub data segment
	 */
	const Data getSubData(const unsigned int start, const unsigned int length) const;

	/**
	 * Compare two data areas and return, if they are equal.
	 * Two data areas are equal, iff they have the same size and the same content.
	 *
	 * @param lhs The left hand side operand
	 * @param rhs The right hand side operand
	 * @return True, iff lhs and rhs are equal.
	 */
	friend bool operator==(const Data &lhs, const Data &rhs);

	/**
	 * Compare two data areas and return, if they are different.
	 * Two data areas are different, iff they have a differnet size or different content.
	 *
	 * @param lhs The left hand side operand
	 * @param rhs The right hand side operand
	 * @return True, iff lhs and rhs are different.
	 */
	friend bool operator!=(const Data &lhs, const Data &rhs);
private:
	std::tr1::shared_ptr<DataBase> _data;
};

#include "impl/Data.impl.hpp"

#endif
