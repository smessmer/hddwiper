#include "../OutputFile.hpp"

int OutputFile::write(const Data data)
{
	size_t written=fwrite(data.get(), 1, data.size(), _file);

	if(written!=data.size())
	{
		//TODO Why is ferror(_file) returning EPERM when "there is no space left on device"?
		if(ferror(_file)!=EPERM)
			throw std::runtime_error("Error writing to output file. ferror: " + IntToStr(ferror(_file)));
	}

	_bytes_written=_bytes_written+written;
	return written;
}
