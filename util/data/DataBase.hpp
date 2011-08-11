#pragma once

#ifndef __DATABASE_HPP__
#define __DATABASE_HPP__

/**
 * An instance of this class represents a specific amount of data.
 * The constructor allocates it and the destructor frees it.
 * This object can't be copied or assigned.
 * This class should only be used by class Data.
 *
 * @author Sebastian Meßmer
 */
class DataBase
{
public:
	/**
	 * Allocate a new data area and create the resource handler object (of class DataBase)
	 *
	 * @param size The size in bytes of the area to allocate
	 */
	DataBase(const unsigned int size);

	/**
	 * Destructor
	 */
	~DataBase();

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
private:
	//Forbid copying
	DataBase(const DataBase &rhs);
	DataBase &operator=(const DataBase &rhs);

	//Pointer to the first byte of the data area
	unsigned char *_data;
	//Size of the data area
	unsigned int _size;
};

/**
 * Compare two data areas and return, if they are equal.
 * Two data areas are equal, iff they have the same size and the same content.
 *
 * @param lhs The left hand side operand
 * @param rhs The right hand side operand
 * @return True, iff lhs and rhs are equal.
 */
bool operator==(const DataBase &lhs, const DataBase &rhs);

/**
 * Compare two data areas and return, if they are different.
 * Two data areas are different, iff they have a differnet size or different content.
 *
 * @param lhs The left hand side operand
 * @param rhs The right hand side operand
 * @return True, iff lhs and rhs are different.
 */
bool operator!=(const DataBase &lhs, const DataBase &rhs);

#include "impl/DataBase.impl.hpp"

#endif
