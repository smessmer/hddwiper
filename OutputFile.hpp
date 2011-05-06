#pragma once

#ifndef __OUTPUTFILE_HPP__
#define __OUTPUTFILE_HPP__

#include <string>
#include <cstdio>

#include "Data.hpp"

//TODO Exceptions instead of cerr
#include <iostream>

class Outputfile
{
public:
	Outputfile(const std::string &filename);

	void write(const Data data);
private:
	FILE *_file;
};

inline Outputfile::Outputfile(const std::string &filename)
	:_file(fopen(filename.c_str(), "wb"))
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
}

#endif
