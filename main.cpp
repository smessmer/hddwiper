#include <iostream>

#include "HDDWiper.hpp"

using namespace std;

// HDDWiper

int main(int argc, char *argv[])
{
	if (argc < 2)
	{
		cout << "Syntax: " << argv[0] << " [targetfile]" << endl;
		return 1;
	}

	HDDWiper wiper(argv[1]);

	while (wiper.isRunning())
	{
		cout.precision(2);
		cout << fixed ;
		cout << "\rWritten: " <<  setw(7) << static_cast<double>(wiper.getBytesWritten())/1024/1024/1024 << " GB"
			 << "\tSpeed: " << setw(7) << wiper.getCurrentSpeed() << " MB/s"
			 << "\tSeedbuffer: " << setw(3) << wiper.getSeedBufferSize()
			 << "\tRandombuffer: " << setw(3) << wiper.getBufferSize()
			 << "\tSeeding: "<< setw(3) << wiper.getSeedingStatus()
			 << flush;
		boost::this_thread::sleep(boost::posix_time::seconds(1));
	}

	return 0;
}


//HDDTester
/*
#include "rc4/RC4Streamgenerator.hpp"
#include "kernelentropy/KernelEntropyProducer.hpp"
#include "file/OutputFile.hpp"
#include "file/InputFile.hpp"
#include <fstream>
int main(int argc, char *argv[])
{
   cout << "Seeding..."<<flush;
   KernelEntropyProducer entropyprod(1,256);
   while(entropyprod.available_count()<1)
   {
     cout << "\rSeeding..."<<entropyprod.seeding_status()<<"/256"<<flush;
     sleep(1);
   }
   Data entropy=entropyprod.get();
   //TODO Remove seedfile output (?)
   {
	   OutputFile seedfile("used.seed");
	   if((int)entropy.size()!=seedfile.write(entropy))
		   cout << "Couldn't write full entropy to disk" << endl;
   }
   cout << "\rSeeding...finished"<<endl;
   RC4Streamgenerator generator(100*1024*1024,entropy);
   Data random(100*1024*1024);
   //Write
   int lastblock_written;
   unsigned long long int sum_written;
   {
    OutputFile out(argv[1]);
    while(true)
    {
      generator.getRandomBytes(random);
      lastblock_written=out.write(random);
      if(lastblock_written!=(int)random.size())
    	  break;
      cout << "\rWritten "<<out.getBytesWritten() << " bytes"<<flush;
    }
    cout << "\rWrite finished: "<<out.getBytesWritten() << " bytes"<<endl;
    sum_written=out.getBytesWritten();
   }
   
   //Check
   int lastblock_read;
   unsigned long long int sum_read;
   generator.reseed(entropy);
   {
     InputFile in(argv[1]);
     Data read(100*1024*1024);
     while(true)
     {
       generator.getRandomBytes(random);
       lastblock_read=in.read(read);
       if(lastblock_read!=(int)random.size())
       {
    	   //Check last (partly available) block
    	   if(lastblock_written!=lastblock_read)
    		   cerr << "Difference in size of the last block!"<<endl;
    	   if(random.getSubData(0,lastblock_read)!=read.getSubData(0,lastblock_read))
    		   cerr << "Read values differ from written values in last block"<<endl;
    	   break;
       }
       if(random!=read)
    	   cerr << "\nRead value differs from written value: " << std::endl;
       cout << "\rRead "<<in.getBytesRead() << " bytes"<<flush;
     }
     cout << "\rRead finished: "<<in.getBytesRead() << " bytes"<<endl;
     sum_read=in.getBytesRead();
   }
   
   if(sum_read!=sum_written)
	   cerr << "Written byte count differs from read byte count"<<endl;

}

*/
