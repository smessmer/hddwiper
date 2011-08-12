#include "util/cast.hpp"

inline OutputFile::OutputFile(const std::string &filename)
	:AbstractFile(filename,"wb"),_bytes_written(0)
{
}

inline void OutputFile::skip(unsigned long long int bytes)
{
	fseeko64(_file,bytes,SEEK_CUR);
}

inline unsigned long long int OutputFile::getBytesWritten() const
{
	return _bytes_written;
}
