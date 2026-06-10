# Sigil Runtime - Linux/macOS Build
#
# Usage:
#   make            - Build libsigil_runtime.a
#   make clean      - Remove build artifacts
#   make test       - Build and run tests (if available)

CC ?= gcc
AR ?= ar
CFLAGS = -O2 -Wall -Wextra -Iinclude -pthread
ARFLAGS = rcs

# Source files
SRCS = src/runtime.c \
       src/task.c \
       src/scheduler.c \
       src/worker.c \
       src/channel.c \
       src/net.c \
       src/file.c \
       src/console.c \
       src/time.c

# Object files in build directory
BUILD_DIR = build
OBJS = $(SRCS:src/%.c=$(BUILD_DIR)/%.o)

# Output library
LIB = $(BUILD_DIR)/libsigil_runtime.a

# Default target
all: $(LIB)

# Create build directory
$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

# Compile source files
$(BUILD_DIR)/%.o: src/%.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Create static library
$(LIB): $(OBJS)
	$(AR) $(ARFLAGS) $@ $^
	@echo "Built $(LIB)"

# Clean build artifacts
clean:
	rm -rf $(BUILD_DIR)

# Debug build
debug: CFLAGS = -g -O0 -Wall -Wextra -Iinclude -pthread -DDEBUG
debug: clean all

# Install (for system-wide use)
PREFIX ?= /usr/local
install: $(LIB)
	install -d $(PREFIX)/lib
	install -d $(PREFIX)/include
	install -m 644 $(LIB) $(PREFIX)/lib/
	install -m 644 include/sigil_runtime.h $(PREFIX)/include/

.PHONY: all clean debug install
