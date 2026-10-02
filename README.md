# Proyecto 2 - Diorama con Raytracing

Diorama voxel inspirado en Zelda que representa el Santuario perdido de Hyrule:
un altar antiguo con la Espada Maestra, camino en ruinas, arbol, estanque y
vegetacion. Esta implementado en Rust con reflexion, refraccion, sombras, cinco
materiales texturizados y skybox procedural.

## Ejecutar

```powershell
cd Raytracing
cargo run --release
```

## Controles

- Flechas: orbitar la camara alrededor del diorama.
- `Q`: acercar la camara.
- `E`: alejar la camara.
- `Esc`: cerrar.

Mientras se mueve la camara se muestra una vista previa rapida. Al soltar las
teclas aparece automaticamente el render completo.

## Texturas propias

El proyecto funciona sin archivos externos mediante texturas procedurales. Para
usar arte propio, consulta [la guia de assets](Raytracing/assets/README.md).

## Video

https://youtu.be/Xlz4sMLvbrg
