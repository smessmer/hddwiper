CPP_FILES := $(shell find . -name "*.cpp")
OBJ_FILES := $(addprefix build/,$(CPP_FILES:.cpp=.o))
LD_FLAGS := -lboost_thread -lboost_system -lboost_program_options -lboost_filesystem -lpthread -lcryptopp
CC_FLAGS := -O3 -I. -std=c++14

hddwiper: $(OBJ_FILES)
	g++ $(CC_FLAGS) -o $@ $^ $(LD_FLAGS)

build/%.o: %.cpp
	mkdir -p $(dir $@)
	g++ $(CC_FLAGS) -c -o $@ $<

clean:
	rm -rf build
	rm hddwiper
