#include "../OutputFile.hpp"

size_t OutputFile::write(const Data data)
{
	size_t written=fwrite(data.get(), 1, data.size(), _file);

	if(written!=data.size())
	{
		if(!ferror(_file))
			throw std::runtime_error("Written too less without error (strange)");
		if(errno!=ENOSPC)
			throw std::runtime_error("Error writing to output file. errno: " + IntToStr(errno));
	}

	_bytes_written=_bytes_written+written;
	return written;
}
