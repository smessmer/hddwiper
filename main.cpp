#include <iostream>
#include <cstdio>
#include <cstring>
#include <sys/time.h>

#include "Profiler.hpp"
#include "RC4Streamgenerator.hpp"

using namespace std;

FILE *getoutput(const std::string &filename)
{
	FILE *output = fopen(filename.c_str(), "wb");
	if (output == NULL)
		cerr << "Error opening output: " << filename << endl;
	return output;
}

void write(const Data &data, FILE *file)
{
	fwrite(data.get(), data.size(), 1, file);
	if (ferror(file))
	{
		cerr << "Error writing to output or finished writing" << endl;
	}
}

void showProgress(unsigned int i)
{
	cout << "\rSeeding from /dev/random: " << i << "/256" << flush;
}

int main(int argc, char *argv[])
{
	if (argc < 2)
	{
		cout << "Syntax: " << argv[0] << " [targetfile]" << endl;
		return 1;
	}

	RC4Streamgenerator generator;

	FILE *output = getoutput(argv[1]);

	unsigned int blocks = 0;
	Data random(RC4Streamgenerator::BLOCKSIZE);
	while (true)
	{
		Profiler time;
		generator.getRandomBytes(random);
		write(random, output);
		++blocks;
		const double mb_per_sec =
				static_cast<double> (RC4Streamgenerator::BLOCKSIZE) / 1024
						/ 1024 / time.getSecondsPassed();
		cout << "\rWritten: " << blocks << " blocks ("
				<< static_cast<double> (blocks)
						* (static_cast<double> (RC4Streamgenerator::BLOCKSIZE)
								/ 1024 / 1024 / 1024) << " GB) " << "Speed: "
				<< mb_per_sec << "MB/s                    " << flush;
		if (blocks % 100 == 0) //10GB bei 100MB Blöcken
		{
			cout << "\n";
			generator.reseed(showProgress);
		}
	}

	return 0;
}
