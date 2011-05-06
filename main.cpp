#include <iostream>

#include "HDDWiper.hpp"

using namespace std;

int main(int argc, char *argv[])
{
	if (argc < 2)
	{
		cout << "Syntax: " << argv[0] << " [targetfile]" << endl;
		return 1;
	}

	HDDWiper wiper(argv[1]);

	while (true)
	{
		cout << "\rWritten: " << static_cast<double>(wiper.getBytesWritten())/1024/1024/1024 << " GB "
			 << "Speed: " << wiper.getCurrentSpeed() << "MB/s "
			 << "Seedbuffer: " << wiper.getSeedBufferSize() << " "
			 << "Randombuffer: " << wiper.getBufferSize()
			 << "Seeding: "<<wiper.getSeedingStatus()
			 << flush;
		sleep(1);
	}

	return 0;
}
