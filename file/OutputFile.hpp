#pragma once

#ifndef __OUTPUTFILE_HPP__
#define __OUTPUTFILE_HPP__

#include "util/data/Data.hpp"
#include "AbstractFile.hpp"
#include "util/thread/Threadsafe.hpp"

class OutputFile: public AbstractFile
{
public:
	OutputFile(const std::string &filename);

	int write(const Data data);

	unsigned long long int getBytesWritten() const;

	void skip(unsigned long long int bytes);

private:
	Threadsafe<unsigned long long int> _bytes_written;
};

#include "impl/OutputFile.impl.hpp"

#endif
