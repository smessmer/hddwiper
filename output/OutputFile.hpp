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

	virtual ~Outputfile();

	int write(const Data data);

	unsigned long long int getBytesWritten() const;

	void skip(unsigned long long int bytes);
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

inline Outputfile::~Outputfile()
{
	if(_file==NULL)
		return;

	if(0!=fclose(_file))
		std::cerr << "Error closing output file"<<std::endl;
	_file=NULL;
}

inline void Outputfile::skip(unsigned long long int bytes)
{
	fseeko64(_file,bytes,SEEK_CUR);
}

inline int Outputfile::write(const Data data)
{
	int written=fwrite(data.get(), 1, data.size(), _file);

	//TODO Handle eof separately, otherwise throw
	//TODO Think about return values. Really return number of written bytes?
	if (ferror(_file))
	{
		std::cerr << "Error writing to output or finished writing" << std::endl;
	}

	_bytes_written=_bytes_written+written;
	return written;
}

inline unsigned long long int Outputfile::getBytesWritten() const
{
	return _bytes_written;
}

#endif
