#include <cstdio>
#include <stdexcept>

inline AbstractFile::AbstractFile(const std::string &filename, const char *mode)
	:_file(fopen(filename.c_str(), mode))
{
	if(NULL==_file)
		throw std::runtime_error("File could not be opened");
}

inline AbstractFile::~AbstractFile()
{
	if(NULL!=_file)
	{
		if(0!=fclose(_file))
			throw std::runtime_error("File could not be closed");
		_file=NULL;
	}
}
