#include <iostream>

#include "util/Profiler.hpp"
#include "rc4/RC4StreamProducer.hpp"
#include "output/OutputFile.hpp"

using namespace std;

void showProgress(unsigned int i)
{
	cout << "\rSeeding from /dev/random: " << i << "/256" << flush;
}

const unsigned int BLOCKSIZE=100*1024*1024;

int main(int argc, char *argv[])
{
	if (argc < 2)
	{
		cout << "Syntax: " << argv[0] << " [targetfile]" << endl;
		return 1;
	}

	RC4StreamProducer generator(5,BLOCKSIZE);
	Outputfile output(argv[1]);

	unsigned int blocks = 0;
	while (true)
	{
		Profiler time;
		Data random=generator.get();
		output.write(random);
		++blocks;
		const double mb_per_sec =
				static_cast<double> (BLOCKSIZE) / 1024
						/ 1024 / time.getSecondsPassed();
		cout << "\rWritten: " << blocks << " blocks ("
				<< static_cast<double> (blocks)
						* (static_cast<double> (BLOCKSIZE)
								/ 1024 / 1024 / 1024) << " GB) " << "Speed: "
				<< mb_per_sec << "MB/s                    " << flush;
	}

	return 0;
}
