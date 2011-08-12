#pragma once

#ifndef ABSTRACTFILE_HPP_
#define ABSTRACTFILE_HPP_

#include <string>

class AbstractFile {
public:
	AbstractFile(const std::string &filename, const char *mode);
	virtual ~AbstractFile();

protected:
	FILE *_file;

private:
	//Forbid copying
	AbstractFile(const AbstractFile &rhs);
	AbstractFile &operator=(const AbstractFile &rhs);
};

#include "impl/AbstractFile.impl.hpp"

#endif /* ABSTRACTFILE_HPP_ */
