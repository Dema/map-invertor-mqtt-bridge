#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define size_of_buffer 560
unsigned char Buffer[size_of_buffer];

void decode_answer(int i)
{
    int c, c1;
    for (c = 1; c <= i; c++)
    {
        if (Buffer[c] == 0xDB && Buffer[c + 1] == 0xDC)
        {
            Buffer[c] = 0x0A;
            --i;
            for (c1 = c; c1 < (i - 1); c1++)
                Buffer[c1 + 1] = Buffer[c1 + 2];
        }
        else if (Buffer[c] == 0xDB && Buffer[c + 1] == 0xDD)
        {
            Buffer[c] = 0xDB;
            --i;
            for (c1 = c; c1 < (i - 1); c1++)
                Buffer[c1 + 1] = Buffer[c1 + 2];
        }
    }
}

int main(int argc, char *argv[]) {
    if (argc != 3) {
        printf("Usage: %s <input_file> <output_file>\n", argv[0]);
        return 1;
    }
    
    // Read input file
    FILE *input = fopen(argv[1], "rb");
    if (!input) {
        printf("Cannot open input file\n");
        return 1;
    }
    
    // Get file size
    fseek(input, 0, SEEK_END);
    long file_size = ftell(input);
    fseek(input, 0, SEEK_SET);
    
    if (file_size > size_of_buffer) {
        printf("File too large\n");
        fclose(input);
        return 1;
    }
    
    // Read data
    size_t bytes_read = fread(Buffer, 1, file_size, input);
    fclose(input);
    
    printf("Read %zu bytes\n", bytes_read);
    printf("Before decode: ");
    for (int i = 0; i < bytes_read && i < 10; i++) {
        printf("%02X ", Buffer[i]);
    }
    printf("\n");
    
    // Decode (assuming last byte is at index bytes_read - 1)
    int last_index = bytes_read - 1;
    decode_answer(last_index);
    
    printf("After decode: ");
    for (int i = 0; i <= last_index; i++) {
        printf("%02X ", Buffer[i]);
    }
    printf("\n");
    
    // Write output file
    FILE *output = fopen(argv[2], "wb");
    if (!output) {
        printf("Cannot open output file\n");
        return 1;
    }
    
    fwrite(Buffer, 1, last_index + 1, output);
    fclose(output);
    
    return 0;
}