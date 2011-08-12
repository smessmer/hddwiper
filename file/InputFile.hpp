#pragma once

#ifndef __INPUTFILE_HPP__
#define __INPUTFILE_HPP__

#include "util/data/Data.hpp"
#include "AbstractFile.hpp"
#include "util/thread/Threadsafe.hpp"

class InputFile: public AbstractFile
{
public:
	InputFile(const std::string &filename);

	size_t read(Data &data);
	
	unsigned long long int getBytesRead() const;

private:
	Threadsafe<unsigned long long int> _bytes_read;
};

#include "impl/InputFile.impl.hpp"

#endif
