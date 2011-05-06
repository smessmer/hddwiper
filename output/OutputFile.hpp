#pragma once

#ifndef __OUTPUTFILE_HPP__
#define __OUTPUTFILE_HPP__

#include <string>
#include <cstdio>

#include "util/data/Data.hpp"

//TODO Exceptions instead of cerr
#include <iostream>

#include "util/thread/Threadsafe.hpp"

class Outputfile
{
public:
	Outputfile(const std::string &filename);

	void write(const Data data);

	unsigned long long int getBytesWritten() const;
private:
	Outputfile(const Outputfile &rhs);
	Outputfile &operator=(const Outputfile &rhs);

	Threadsafe<unsigned long long int> _bytes_written;
	FILE *_file;
};

inline Outputfile::Outputfile(const std::string &filename)
	:_bytes_written(0),_file(fopen(filename.c_str(), "wb"))
{
	if(_file==NULL)
		std::cerr << "Error opening output file" <<std::endl;
}

inline void Outputfile::write(const Data data)
{
	fwrite(data.get(), data.size(), 1, _file);
	if (ferror(_file))
	{
		std::cerr << "Error writing to output or finished writing" << std::endl;
	}

	_bytes_written=_bytes_written+data.size();
}

inline unsigned long long int Outputfile::getBytesWritten() const
{
	return _bytes_written;
}

#endif
