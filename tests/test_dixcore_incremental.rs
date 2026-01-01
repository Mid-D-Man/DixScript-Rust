//! tests/test_dixcore_incremental.rs
//! Comprehensive testing for DixCore and Utilities with debug logging

#[cfg(test)]
mod tests {
    use dixscript::DixCore::{Dictionary, HashSet, ImmutableArray, Linq, List};
    use dixscript::Utilities::{
        Keywords, LogLevel, MID_HelperFunctions, MID_Logger, StringExtensions, Token, TokenType,
    };

    // ========== MID_HelperFunctions Tests ==========

    #[test]
    fn test_is_valid_string() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing IsValidString");

        assert!(MID_HelperFunctions::IsValidString("hello"));
        logger.Debug("✓ 'hello' is valid");

        assert!(MID_HelperFunctions::IsValidString("  hello  "));
        logger.Debug("✓ '  hello  ' (with spaces) is valid");

        assert!(!MID_HelperFunctions::IsValidString(""));
        logger.Debug("✓ Empty string is invalid");

        assert!(!MID_HelperFunctions::IsValidString("NULL"));
        logger.Debug("✓ 'NULL' is invalid");

        assert!(!MID_HelperFunctions::IsValidString("undefined"));
        logger.Debug("✓ 'undefined' is invalid");

        logger.Info("IsValidString tests passed!");
    }

    #[test]
    fn test_get_environment() {
        let logger = MID_Logger::New(LogLevel::Info, true);
        logger.Info("Testing GetEnvironment");

        let env = MID_HelperFunctions::GetEnvironment();
        logger.Info(&format!("Current environment: {}", env));

        #[cfg(debug_assertions)]
        {
            assert_eq!(env, "Development");
            logger.Debug("✓ Running in Development mode");
        }

        #[cfg(not(debug_assertions))]
        {
            assert_eq!(env, "Production");
            logger.Debug("✓ Running in Production mode");
        }
    }

    #[test]
    fn test_generate_random_string() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing GenerateRandomString");

        let random_str = MID_HelperFunctions::GenerateRandomString(10, false);
        logger.Debug(&format!("Generated string (no special): {}", random_str));
        assert_eq!(random_str.len(), 10);

        let random_str_special = MID_HelperFunctions::GenerateRandomString(20, true);
        logger.Debug(&format!("Generated string (with special): {}", random_str_special));
        assert_eq!(random_str_special.len(), 20);

        let empty = MID_HelperFunctions::GenerateRandomString(0, false);
        logger.Debug("Generated empty string");
        assert_eq!(empty.len(), 0);

        logger.Info("GenerateRandomString tests passed!");
    }

    // ========== List Tests ==========

    #[test]
    fn test_list_basic_operations() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing List basic operations");

        let mut list = List::New();
        logger.Debug("Created new List");

        list.Add(1);
        list.Add(2);
        list.Add(3);
        logger.Debug(&format!("Added 3 items, count: {}", list.Count()));

        assert_eq!(list.Count(), 3);
        assert_eq!(list[0], 1);
        assert_eq!(list[2], 3);
        logger.Debug("✓ List indexing works");

        assert!(list.Contains(&2));
        logger.Debug("✓ List Contains() works");

        logger.Info("List basic operations tests passed!");
    }

    #[test]
    fn test_list_remove() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing List Remove");

        let mut list = List::New();
        list.AddRange(vec![1, 2, 3, 4, 5]);
        logger.Debug(&format!("Created list with {} items", list.Count()));

        let removed = list.Remove(&3);
        logger.Debug(&format!("Removed item 3: {}", removed));
        assert!(removed);
        assert_eq!(list.Count(), 4);
        assert!(!list.Contains(&3));

        logger.Info("List Remove tests passed!");
    }

    #[test]
    fn test_list_linq_methods() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing List LINQ methods");

        let list = List::From(vec![1, 2, 3, 4, 5]);
        logger.Debug(&format!("Created list with {} items", list.Count()));

        {
            let _scope = logger.CreateScope("Testing Select");
            let doubled = list.Select(|x| x * 2);
            logger.Debug(&format!("Doubled first item: {}", doubled[0]));
            assert_eq!(doubled[0], 2);
        }

        {
            let _scope = logger.CreateScope("Testing Where");
            let evens = list.Where(|x| x % 2 == 0);
            logger.Debug(&format!("Found {} even numbers", evens.Count()));
            assert_eq!(evens.Count(), 2);
        }

        logger.Info("List LINQ methods tests passed!");
    }

    // ========== LINQ Tests ==========

    #[test]
    fn test_linq_select() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Linq::Select");

        let numbers = vec![1, 2, 3, 4, 5];
        logger.Debug(&format!("Input: {:?}", numbers));

        let doubled = Linq::Select(numbers, |x| x * 2);
        logger.Debug(&format!("Output count: {}", doubled.Count()));
        logger.Debug(&format!("First: {}, Last: {}", doubled[0], doubled[4]));

        assert_eq!(doubled.Count(), 5);
        assert_eq!(doubled[0], 2);
        assert_eq!(doubled[4], 10);

        logger.Info("Linq::Select tests passed!");
    }

    #[test]
    fn test_linq_where() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Linq::Where");

        let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        logger.Debug(&format!("Input: {} numbers", numbers.len()));

        let evens = Linq::Where(numbers, |x| x % 2 == 0);
        logger.Debug(&format!("Found {} even numbers", evens.Count()));

        assert_eq!(evens.Count(), 5);
        assert!(evens.All(|x| x % 2 == 0));

        logger.Info("Linq::Where tests passed!");
    }

    #[test]
    fn test_linq_order_by() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Linq::OrderBy");

        let numbers = vec![5, 2, 8, 1, 9, 3];
        logger.Debug(&format!("Input (unsorted): {:?}", numbers));

        let sorted = Linq::OrderBy(numbers, |x| *x);
        logger.Debug(&format!("Output (sorted): First={}, Last={}", sorted[0], sorted[5]));

        assert_eq!(sorted[0], 1);
        assert_eq!(sorted[5], 9);

        logger.Info("Linq::OrderBy tests passed!");
    }

    #[test]
    fn test_linq_take_skip() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Linq::Take and Skip");

        let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        logger.Debug(&format!("Input: {} numbers", numbers.len()));

        {
            let _scope = logger.CreateScope("Testing Take");
            let first_five = Linq::Take(numbers.clone(), 5);
            logger.Debug(&format!("Took first 5: count={}", first_five.Count()));
            assert_eq!(first_five.Count(), 5);
            assert_eq!(first_five[0], 1);
        }

        {
            let _scope = logger.CreateScope("Testing Skip");
            let skip_five = Linq::Skip(numbers, 5);
            logger.Debug(&format!("Skipped first 5: count={}, first={}", skip_five.Count(), skip_five[0]));
            assert_eq!(skip_five.Count(), 5);
            assert_eq!(skip_five[0], 6);
        }

        logger.Info("Linq::Take/Skip tests passed!");
    }

    #[test]
    fn test_linq_distinct() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Linq::Distinct");

        let numbers = vec![1, 2, 2, 3, 3, 3, 4, 4, 4, 4];
        logger.Debug(&format!("Input: {} numbers (with duplicates)", numbers.len()));

        let unique = Linq::Distinct(numbers);
        logger.Debug(&format!("Output: {} unique numbers", unique.Count()));

        assert_eq!(unique.Count(), 4);

        logger.Info("Linq::Distinct tests passed!");
    }

    #[test]
    fn test_linq_group_by() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Linq::GroupBy");

        let words = vec!["apple", "banana", "apricot", "blueberry", "avocado"];
        logger.Debug(&format!("Input: {} words", words.len()));

        let grouped = Linq::GroupBy(words, |w| w.chars().next().unwrap());
        logger.Debug(&format!("Grouped into {} groups", grouped.Count()));

        assert_eq!(grouped.Count(), 2);
        assert!(grouped.ContainsKey(&'a'));
        assert!(grouped.ContainsKey(&'b'));

        let a_words = grouped.Get(&'a').unwrap();
        logger.Debug(&format!("Words starting with 'a': {}", a_words.Count()));
        assert_eq!(a_words.Count(), 3);

        logger.Info("Linq::GroupBy tests passed!");
    }

    // ========== Dictionary Tests ==========

    #[test]
    fn test_dictionary_basic() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Dictionary basic operations");

        let mut dict = Dictionary::New();
        logger.Debug("Created new Dictionary");

        dict.Add("key1", "value1");
        dict.Add("key2", "value2");
        logger.Debug(&format!("Added {} items", dict.Count()));

        assert_eq!(dict.Count(), 2);
        assert!(dict.ContainsKey(&"key1"));
        assert_eq!(dict.Get(&"key1"), Some(&"value1"));

        logger.Info("Dictionary basic operations tests passed!");
    }

    #[test]
    fn test_dictionary_try_get_value() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Dictionary TryGetValue");

        let mut dict = Dictionary::New();
        dict.Add("name", "DixScript");
        dict.Add("version", "1.0");
        logger.Debug("Created dictionary with name and version");

        {
            let _scope = logger.CreateScope("Testing existing key");
            assert!(dict.TryGetValue(&"name").is_some());
            assert_eq!(dict.TryGetValue(&"name"), Some(&"DixScript"));
            logger.Debug("✓ Found 'name' key");
        }

        {
            let _scope = logger.CreateScope("Testing missing key");
            assert!(dict.TryGetValue(&"missing").is_none());
            logger.Debug("✓ 'missing' key not found");
        }

        logger.Info("Dictionary TryGetValue tests passed!");
    }

    // ========== HashSet Tests ==========

    #[test]
    fn test_hashset_basic() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing HashSet basic operations");

        let mut set = HashSet::New();
        logger.Debug("Created new HashSet");

        assert!(set.Add(1));
        logger.Debug("Added 1");

        assert!(set.Add(2));
        logger.Debug("Added 2");

        assert!(!set.Add(1)); // Duplicate
        logger.Debug("Attempted to add duplicate 1 (rejected)");

        assert_eq!(set.Count(), 2);
        assert!(set.Contains(&1));

        logger.Info("HashSet basic operations tests passed!");
    }

    #[test]
    fn test_hashset_operations() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing HashSet set operations");

        let set1 = HashSet::from_iter(vec![1, 2, 3]);
        let set2 = HashSet::from_iter(vec![2, 3, 4]);
        logger.Debug("Created two sets: [1,2,3] and [2,3,4]");

        {
            let _scope = logger.CreateScope("Testing Union");
            let union = set1.UnionWith(&set2);
            logger.Debug(&format!("Union count: {}", union.Count()));
            assert_eq!(union.Count(), 4);
        }

        {
            let _scope = logger.CreateScope("Testing Intersection");
            let intersection = set1.IntersectWith(&set2);
            logger.Debug(&format!("Intersection count: {}", intersection.Count()));
            assert_eq!(intersection.Count(), 2);
        }

        logger.Info("HashSet set operations tests passed!");
    }

    // ========== ImmutableArray Tests ==========

    #[test]
    fn test_immutable_array() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing ImmutableArray");

        let arr = ImmutableArray::Create(vec![1, 2, 3, 4, 5]);
        logger.Debug(&format!("Created ImmutableArray with {} items", arr.Length()));

        assert_eq!(arr.Length(), 5);
        assert_eq!(arr[0], 1);
        logger.Debug(&format!("First element: {}", arr[0]));

        assert_eq!(arr.Get(2), Some(&3));
        logger.Debug("✓ Get(2) returns Some(3)");

        assert!(arr.Get(10).is_none());
        logger.Debug("✓ Get(10) returns None");

        logger.Info("ImmutableArray tests passed!");
    }

    // ========== Token Tests ==========

    #[test]
    fn test_token_creation() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Token creation");

        let token = Token::New(TokenType::Integer(42), 1, 5);
        logger.Debug(&format!("Created token: {}", token.ToString()));

        assert_eq!(token.Line, 1);
        assert_eq!(token.Column, 5);
        assert!(matches!(token.TokenType, TokenType::Integer(42)));

        logger.Info("Token creation tests passed!");
    }

    #[test]
    fn test_token_with_section() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing Token with section");

        let token = Token::New(TokenType::Keyword("if".to_string()), 10, 2);
        logger.Debug(&format!("Created token: {}", token.ToString()));

        let with_section = token.WithSection("DATA".to_string());
        logger.Debug(&format!("Token with section: {}", with_section.ToString()));

        assert_eq!(with_section.Section, Some("DATA".to_string()));

        logger.Info("Token with section tests passed!");
    }

    // ========== Keywords Tests ==========

    #[test]
    fn test_keywords_truly_reserved() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing truly reserved keywords");

        assert!(Keywords::IsTrulyReservedKeyword("if"));
        logger.Debug("✓ 'if' is truly reserved");

        assert!(Keywords::IsTrulyReservedKeyword("else"));
        logger.Debug("✓ 'else' is truly reserved");

        assert!(Keywords::IsTrulyReservedKeyword("return"));
        logger.Debug("✓ 'return' is truly reserved");

        assert!(!Keywords::IsTrulyReservedKeyword("int"));
        logger.Debug("✓ 'int' is NOT truly reserved (data type)");

        logger.Info("Keywords truly reserved tests passed!");
    }

    #[test]
    fn test_keywords_data_type() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing data type keywords");

        assert!(Keywords::IsDataTypeKeyword("int"));
        logger.Debug("✓ 'int' is a data type keyword");

        assert!(Keywords::IsDataTypeKeyword("string"));
        logger.Debug("✓ 'string' is a data type keyword");

        assert!(!Keywords::IsDataTypeKeyword("if"));
        logger.Debug("✓ 'if' is NOT a data type keyword");

        logger.Info("Keywords data type tests passed!");
    }

    #[test]
    fn test_keywords_context_aware() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing context-aware keyword validation");

        {
            let _scope = logger.CreateScope("Data type keywords in DATA section");
            let is_reserved = Keywords::IsReservedInContext("int", "DATA");
            logger.Debug(&format!("'int' reserved in DATA: {}", is_reserved));
            assert!(!is_reserved);
        }

        {
            let _scope = logger.CreateScope("Truly reserved keywords always reserved");
            assert!(Keywords::IsReservedInContext("if", "DATA"));
            logger.Debug("✓ 'if' reserved in DATA");

            assert!(Keywords::IsReservedInContext("if", "CONFIG"));
            logger.Debug("✓ 'if' reserved in CONFIG");
        }

        logger.Info("Keywords context-aware tests passed!");
    }

    // ========== Logger Tests ==========

    #[test]
    fn test_logger_basic() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing MID_Logger basic operations");

        logger.Debug("Debug message");
        logger.Info("Info message");
        logger.Warning("Warning message");
        logger.Error("Error message");

        let contents = logger.GetLogContents();
        assert!(contents.contains("Debug message"));
        assert!(contents.contains("Info message"));

        logger.Info("Logger basic operations tests passed!");
    }

    #[test]
    fn test_logger_levels() {
        let logger = MID_Logger::New(LogLevel::Warning, true);
        logger.Info("Testing MID_Logger level filtering");

        logger.Debug("Should not appear");
        logger.Info("Should not appear");
        logger.Warning("Should appear");
        logger.Error("Should appear");

        let contents = logger.GetLogContents();
        assert!(!contents.contains("Should not appear"));
        assert!(contents.contains("Should appear"));

        logger.Warning("Logger level filtering tests passed!");
    }

    #[test]
    fn test_logger_scope() {
        let logger = MID_Logger::New(LogLevel::Info, true);
        logger.Info("Testing MID_Logger scopes");

        logger.Info("Before scope");
        {
            let _scope = logger.CreateScope("Test Scope");
            logger.Info("Inside scope");
        }
        logger.Info("After scope");

        let contents = logger.GetLogContents();
        assert!(contents.contains("▶ Test Scope"));
        assert!(contents.contains("◀ Test Scope"));

        logger.Info("Logger scope tests passed!");
    }

    // ========== StringExtensions Tests ==========

    #[test]
    fn test_string_extensions() {
        let logger = MID_Logger::New(LogLevel::Debug, true);
        logger.Info("Testing StringExtensions");

        {
            let _scope = logger.CreateScope("Testing Split");
            let parts = StringExtensions::Split("a,b,c", ',');
            logger.Debug(&format!("Split result: {:?}", parts));
            assert_eq!(parts.len(), 3);
            assert_eq!(parts[0], "a");
        }

        {
            let _scope = logger.CreateScope("Testing Join");
            let parts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
            let joined = StringExtensions::Join(",", &parts);
            logger.Debug(&format!("Join result: {}", joined));
            assert_eq!(joined, "a,b,c");
        }

        {
            let _scope = logger.CreateScope("Testing StartsWith/EndsWith");
            assert!(StringExtensions::StartsWith("hello", "he"));
            logger.Debug("✓ 'hello' starts with 'he'");

            assert!(StringExtensions::EndsWith("hello", "lo"));
            logger.Debug("✓ 'hello' ends with 'lo'");
        }

        logger.Info("StringExtensions tests passed!");
    }
}