#!/bin/bash

# Function to convert PascalCase to snake_case
to_snake_case() {
    echo "$1" | sed -e 's/\([A-Z]\)/_\1/g' -e 's/^_//' | tr '[:upper:]' '[:lower:]'
}

# Find all .rs files in src directory, excluding mod.rs and lib.rs
find src -type f -name "*.rs" ! -name "mod.rs" ! -name "lib.rs" | while read -r file; do
    # Get directory and filename
    dir=$(dirname "$file")
    filename=$(basename "$file" .rs)
    
    # Check if filename contains uppercase letters
    if [[ "$filename" =~ [A-Z] ]]; then
        # Convert to snake_case
        new_filename=$(to_snake_case "$filename")
        new_path="$dir/${new_filename}.rs"
        
        # Show what we're doing
        echo "Renaming: $file -> $new_path"
        
        # Rename the file
        mv "$file" "$new_path"
    fi
done

echo "Done! All files renamed to snake_case."
