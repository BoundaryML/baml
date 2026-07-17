namespace Baml.Bridge;

internal interface IBamlStreamValue
{
    (ulong Key, int HandleType) CloneForWire();
}
