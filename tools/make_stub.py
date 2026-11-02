import re
src=open('DixScript-Rust/mdix-csharp/src/MidManStudio.Mdix.Core/MdixDatabase.cs').read()
names=['GetValueType','GetString','GetInt','GetLong','GetFloat','GetDouble','GetBool','GetHexColor','GetBlob','GetRegex','GetDate','GetTimestamp','GetEnumName','GetEnumField','GetArrayLength','GetKeys','GetJson','Exists','GetTupleRaw']
sigs=[]
for n in names:
    m=re.search(r'^\s*public (?!static)([^=\n{]*?\b'+n+r'\([^)\n]*\))',src,re.M)
    assert m,n
    sigs.append(m.group(1).strip())
enum=re.search(r'public enum MdixValueType\s*\{.*?\n    \}',src,re.S).group(0)
out='''using System;
#nullable enable
namespace MidManStudio.Mdix.Core
{
    %s

    // Signatures below are copied mechanically from the real MdixDatabase.cs; bodies are stubs.
    public sealed class MdixDatabase : IDisposable
    {
        public void Dispose() {}
%s
    }
}
namespace MidManStudio.Mdix
{
    public static class Dix
    {
        public static Core.MdixResult<Core.MdixDatabase> LoadStr(string source) => throw new NotImplementedException();
    }
}
''' % (enum, '\n'.join('        public %s => throw new NotImplementedException();'%s for s in sigs))
open('build/stub.cs','w').write(out)
print("stub with", len(sigs), "signatures")
